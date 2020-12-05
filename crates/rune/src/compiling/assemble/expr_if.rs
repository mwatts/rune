use crate::compiling::assemble::prelude::*;

/// Compile an if expression.
impl Assemble for ast::ExprIf {
    fn assemble(&self, c: &mut Compiler<'_>, needs: Needs) -> CompileResult<Value> {
        let span = self.span();
        log::trace!("ExprIf => {:?}", c.source.source(span));

        let then_label = c.asm.new_label("if_then");
        let end_label = c.asm.new_label("if_end");

        let mut branches = Vec::new();
        let then_scope = c.compile_condition(&self.condition, then_label)?;

        for branch in &self.expr_else_ifs {
            let label = c.asm.new_label("if_branch");
            let scope = c.compile_condition(&branch.condition, label)?;
            branches.push((branch, label, scope));
        }

        // use fallback as fall through.
        if let Some(fallback) = &self.expr_else {
            fallback.block.assemble(c, needs)?;
        } else {
            // NB: if we must produce a value and there is no fallback branch,
            // encode the result of the statement as a unit.
            if needs.value() {
                c.asm.push(Inst::unit(), span);
            }
        }

        c.asm.jump(end_label, span);

        c.asm.label(then_label)?;

        let guard = c.scopes.push_scope(span, then_scope)?;
        self.block.assemble(c, needs)?;
        c.locals_clean(span, needs)?;
        let scope = guard.pop(span, c)?;
        debug_assert!(scope.len() == needs.transfer());

        if !self.expr_else_ifs.is_empty() {
            c.asm.jump(end_label, span);
        }

        let mut it = branches.into_iter().peekable();

        if let Some((branch, label, scope)) = it.next() {
            let span = branch.span();

            c.asm.label(label)?;

            // NB: we do some temporary scope switcheroo here, since the scopes
            // have been prepared beforehand.
            let guard = c.scopes.push_scope(span, scope)?;
            branch.block.assemble(c, needs)?;
            c.locals_clean(span, needs)?;
            let scope = guard.pop(span, c)?;
            debug_assert!(scope.len() == needs.transfer());

            if it.peek().is_some() {
                c.asm.jump(end_label, span);
            }
        }

        c.asm.label(end_label)?;

        if !needs.value() {
            return Ok(Value::empty(span));
        }

        Ok(Value::unnamed(span, c))
    }
}
