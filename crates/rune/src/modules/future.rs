//! Asynchronous computations.

use crate as rune;
use crate::alloc::Vec;
use crate::runtime::{
    Future, Inline, Mut, Mutable, SelectFuture, Value, ValueBorrowRef, ValueRef, VmErrorKind,
    VmResult,
};
use crate::{ContextError, Module};

/// Asynchronous computations.
#[rune::module(::std::future)]
pub fn module() -> Result<Module, ContextError> {
    let mut module = Module::from_meta(self::module_meta)?;
    module.ty::<Future>()?;
    module.function_meta(join)?;
    Ok(module)
}

async fn try_join_impl<'a, I, F>(values: I, len: usize, factory: F) -> VmResult<Value>
where
    I: IntoIterator<Item = &'a Value>,
    F: FnOnce(Vec<Value>) -> VmResult<Value>,
{
    use futures_util::stream::StreamExt as _;

    let mut futures = futures_util::stream::FuturesUnordered::new();
    let mut results = vm_try!(Vec::try_with_capacity(len));

    for (index, value) in values.into_iter().enumerate() {
        let value = match vm_try!(value.value_ref()) {
            ValueRef::Mutable(value) => vm_try!(value.clone().into_mut()),
            ValueRef::Inline(actual) => {
                return VmResult::err([
                    VmErrorKind::expected::<Future>(actual.type_info()),
                    VmErrorKind::bad_argument(index),
                ]);
            }
        };

        let future = Mut::try_map(value, |kind| match kind {
            Mutable::Future(future) => Some(future),
            _ => None,
        });

        let future = match future {
            Ok(future) => future,
            Err(actual) => {
                return VmResult::err([
                    VmErrorKind::expected::<Future>(actual.type_info()),
                    VmErrorKind::bad_argument(index),
                ]);
            }
        };

        futures.push(SelectFuture::new(index, future));
        vm_try!(results.try_push(Value::empty()));
    }

    while !futures.is_empty() {
        let (index, value) = vm_try!(futures.next().await.unwrap());
        *results.get_mut(index).unwrap() = value;
    }

    factory(results)
}

/// Waits for a collection of futures to complete and joins their result.
///
/// # Examples
///
/// ```rune
/// let a = async { 1 };
/// let b = async { 2 };
/// let (a, b) = std::future::join((a, b)).await;
/// assert_eq!(1, a);
/// assert_eq!(2, b);
/// ```
///
/// Using a vector:
///
/// ```rune
/// let a = async { 1 };
/// let b = async { 2 };
/// let [a, b] = std::future::join([a, b]).await;
/// assert_eq!(1, a);
/// assert_eq!(2, b);
/// ```
///
/// Joining an empty collection:
///
/// ```rune
/// let () = std::future::join(()).await;
/// let [] = std::future::join([]).await;
/// ```
#[rune::function]
async fn join(value: Value) -> VmResult<Value> {
    match vm_try!(value.borrow_ref()) {
        ValueBorrowRef::Inline(value) => match value {
            Inline::Unit => VmResult::Ok(Value::unit()),
            value => VmResult::err([
                VmErrorKind::bad_argument(0),
                VmErrorKind::expected::<crate::runtime::Vec>(value.type_info()),
            ]),
        },
        ValueBorrowRef::Mutable(value) => match *value {
            Mutable::Tuple(ref tuple) => VmResult::Ok(vm_try!(
                try_join_impl(tuple.iter(), tuple.len(), |vec| VmResult::Ok(vm_try!(
                    Value::tuple(vec)
                )))
                .await
            )),
            Mutable::Vec(ref vec) => VmResult::Ok(vm_try!(
                try_join_impl(vec.iter(), vec.len(), Value::vec).await
            )),
            ref value => VmResult::err([
                VmErrorKind::bad_argument(0),
                VmErrorKind::expected::<crate::runtime::Vec>(value.type_info()),
            ]),
        },
    }
}
