//! Runtime module values and value-qualified call resolution.

use crate::bytecode::{Instr, ModuleOperands, ValueType};
use crate::compile::{CResult, CoreCompiler};
use crate::ir::core::Expr;

impl CoreCompiler<'_> {
    pub(in crate::compile) fn emit_module_value(&mut self, module_name: &str) {
        // Module declarations materialize their authoritative runtime value
        // (including `public` metadata) before main executes. Loading that
        // binding keeps reflection on `TestMod` attached to the declared
        // ModuleValue instead of synthesizing a metadata-poor copy here.
        // Restrict this to main-scope top-level bindings: a module body can run
        // before its own value is stored, and builtin/nested module paths do
        // not necessarily have a same-named global slot.
        if self.current_module_path.is_none() && self.toplevel_module_bindings.contains(module_name)
        {
            self.emit(Instr::LoadGlobalAny(module_name.to_string()));
            return;
        }

        let exports = self
            .module_exports
            .get(module_name)
            .map(|set| {
                let mut exports: Vec<String> = set.iter().cloned().collect();
                exports.sort();
                exports
            })
            .unwrap_or_default();
        let publics = self
            .shared_ctx
            .module_publics
            .get(module_name)
            .cloned()
            .unwrap_or_default();
        self.emit(Instr::PushModule(Box::new(ModuleOperands {
            name: module_name.to_string(),
            exports,
            publics,
            base_exports_visible: true,
            implicit_standard_bindings: true,
        })));
    }

    /// Resolve a module reference to its canonical name, applying module aliases.
    pub(in crate::compile) fn resolve_module_alias_path(&self, module: &str) -> String {
        if let Some(resolved) = self.resolved_module_alias(module) {
            return resolved.to_string();
        }
        if let Some((root, rest)) = module.split_once('.') {
            if let Some(resolved_root) = self.resolved_module_alias(root) {
                return format!("{}.{}", resolved_root, rest);
            }
        }
        module.to_string()
    }

    /// Compile a module-qualified function call: Module.func(args)
    pub(in crate::compile) fn compile_module_call(
        &mut self,
        module: &str,
        function: &str,
        args: &[Expr],
        kwargs: &[(crate::ir::core::InternedStr, Expr)],
        splat_mask: &[bool],
        kwargs_splat_mask: &[bool],
    ) -> CResult<ValueType> {
        let root = module.split('.').next().unwrap_or(module);
        let local_root_shadows = self.locals.get(root).is_some_and(|ty| {
            !matches!(ty, ValueType::Module)
                || (self.initialized_locals.contains(root)
                    && !self.module_aliases.contains_key(root))
        }) && (self.strict_undefined_check
            || self.initialized_locals.contains(root));
        let global_root_shadows = self.shared_ctx.global_types.contains_key(root)
            && !self.imported_bindings.contains(root)
            && !self.module_aliases.contains_key(root)
            && !self.module_alias_states.contains_key(root);
        if !self.is_renamed_only_module_root(root)
            && (self.explicit_lexical_owner_active(root)
                || local_root_shadows
                || self.captured_vars.contains(root)
                || global_root_shadows)
        {
            return self.compile_value_qualified_call(
                module,
                function,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                false,
            );
        }

        let owned_module_path = self.module_path_in_current_scope(module);
        if owned_module_path.is_none()
            && !crate::compile::constants::is_stdlib_module(root)
            && self.imported_binding_root(module).is_some()
        {
            return self.compile_imported_module_alias_call(
                module,
                function,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
            );
        }

        if let Some(resolved_module) = self.resolve_visible_module_path(module) {
            return self.compile_resolved_module_call(
                &resolved_module,
                function,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
            );
        }

        // Unknown roots can still be runtime Module values. Statically-known
        // but unbound roots must instead raise the lexical UndefVarError.
        let canonical = self.resolve_module_alias_path(module);
        let canonical_root = canonical.split('.').next().unwrap_or(&canonical);
        let statically_known_unbound = self.is_known_module_path(&canonical)
            || self.is_known_module_path(canonical_root)
            || self.is_renamed_only_module_root(root)
            || self.is_ambiguous_module_alias_root(root);
        self.compile_value_qualified_call(
            module,
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            statically_known_unbound,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn compile_value_qualified_call(
        &mut self,
        module: &str,
        function: &str,
        args: &[Expr],
        kwargs: &[(crate::ir::core::InternedStr, Expr)],
        splat_mask: &[bool],
        kwargs_splat_mask: &[bool],
        unbound_root: bool,
    ) -> CResult<ValueType> {
        let mut segments = module.split('.');
        let root = segments.next().unwrap_or(module);
        if unbound_root {
            self.emit_unbound_module_name(root);
        } else if self.captured_vars.contains(root) && !self.locals.contains_key(root) {
            self.emit(Instr::LoadCaptured(root.to_string()));
        } else {
            self.emit(Instr::LoadAny(root.to_string()));
        }
        for segment in segments {
            self.emit(Instr::GetFieldByName(segment.to_string()));
        }
        self.emit(Instr::GetFieldByName(function.to_string()));
        let callee = self.new_temp("qualified_callee");
        self.emit(Instr::StoreAny(callee.clone()));

        for arg in args {
            self.compile_expr(arg)?;
        }
        let kwarg_names: Vec<String> = kwargs.iter().map(|(name, _)| name.to_string()).collect();
        for (_, value) in kwargs {
            self.compile_expr(value)?;
        }
        self.emit(Instr::LoadAny(callee));

        let has_splat = splat_mask.iter().any(|is_splat| *is_splat);
        let has_kwargs = !kwargs.is_empty();
        let has_kwargs_splat = kwargs_splat_mask.iter().any(|is_splat| *is_splat);
        if has_splat || has_kwargs || has_kwargs_splat {
            self.emit(Instr::CallFunctionVariableWithKwargsSplat(Box::new(
                crate::bytecode::CallVarKwargsSplat {
                    arg_count: args.len(),
                    pos_splat_mask: splat_mask.to_vec(),
                    kwarg_names,
                    kwargs_splat_mask: kwargs_splat_mask.to_vec(),
                },
            )));
        } else {
            self.emit(Instr::CallFunctionVariable(args.len()));
        }
        Ok(ValueType::Any)
    }
}
