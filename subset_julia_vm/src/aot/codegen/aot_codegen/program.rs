use super::AotCodeGenerator;
use crate::aot::abi::AOT_RUNTIME_ABI_VERSION;
use crate::aot::ir::{AotEnum, AotExpr, AotFunction, AotGlobal, AotProgram, AotStmt, AotStruct};
use crate::aot::types::StaticType;
use crate::aot::{AotError, AotResult, UnsupportedInstructionDiagnostic};
use std::collections::{HashMap, HashSet};

use super::escape_rust_ident;
use super::global_static_ident;

mod c_abi;

/// Collect the names of all variables *read* by an expression. Used by the
/// branch-escaping-local hoist analysis (Issue #8181). Exhaustive over
/// `AotExpr` (no wildcard arm) so a new variant forces this to be revisited.
fn collect_expr_reads(expr: &AotExpr, out: &mut HashSet<String>) {
    match expr {
        AotExpr::Var { name, .. } => {
            out.insert(name.clone());
        }
        AotExpr::BinOpStatic { left, right, .. } | AotExpr::BinOpDynamic { left, right, .. } => {
            collect_expr_reads(left, out);
            collect_expr_reads(right, out);
        }
        AotExpr::UnaryOp { operand, .. } => collect_expr_reads(operand, out),
        AotExpr::CallStatic { args, .. }
        | AotExpr::CallDynamic { args, .. }
        | AotExpr::CallBuiltin { args, .. }
        | AotExpr::ArrayLit { elements: args, .. }
        | AotExpr::TupleLit { elements: args }
        | AotExpr::StructNew { fields: args, .. } => {
            for arg in args {
                collect_expr_reads(arg, out);
            }
        }
        AotExpr::SetFromIter { iter, .. } => collect_expr_reads(iter, out),
        AotExpr::NamedTupleLit { fields } => {
            for (_, field) in fields {
                collect_expr_reads(field, out);
            }
        }
        AotExpr::Comprehension {
            body, iter, filter, ..
        }
        | AotExpr::Generator {
            body, iter, filter, ..
        } => {
            collect_expr_reads(iter, out);
            if let Some(filter) = filter {
                collect_expr_reads(filter, out);
            }
            collect_expr_reads(body, out);
        }
        AotExpr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            for (_, iter) in iterations {
                collect_expr_reads(iter, out);
            }
            if let Some(filter) = filter {
                collect_expr_reads(filter, out);
            }
            collect_expr_reads(body, out);
        }
        AotExpr::Index { array, indices, .. } => {
            collect_expr_reads(array, out);
            for index in indices {
                collect_expr_reads(index, out);
            }
        }
        AotExpr::Range {
            start, stop, step, ..
        } => {
            collect_expr_reads(start, out);
            collect_expr_reads(stop, out);
            if let Some(step) = step {
                collect_expr_reads(step, out);
            }
        }
        AotExpr::FieldAccess { object, .. } => collect_expr_reads(object, out),
        AotExpr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            collect_expr_reads(condition, out);
            collect_expr_reads(then_expr, out);
            collect_expr_reads(else_expr, out);
        }
        AotExpr::Box(inner)
        | AotExpr::Unbox { value: inner, .. }
        | AotExpr::Convert { value: inner, .. } => collect_expr_reads(inner, out),
        AotExpr::Lambda { body, captures, .. } => {
            for (name, _) in captures {
                out.insert(name.clone());
            }
            collect_stmt_reads(body, out);
        }
        AotExpr::LitI64(_)
        | AotExpr::LitI32(_)
        | AotExpr::LitF64(_)
        | AotExpr::LitF32(_)
        | AotExpr::LitBool(_)
        | AotExpr::LitStr(_)
        | AotExpr::LitChar(_)
        | AotExpr::LitNothing
        | AotExpr::LitMissing => {}
    }
}

fn collect_stmt_reads(stmts: &[AotStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            AotStmt::Let { value, .. } => collect_expr_reads(value, out),
            AotStmt::Assign { target, value } | AotStmt::CompoundAssign { target, value, .. } => {
                collect_expr_reads(target, out);
                collect_expr_reads(value, out);
            }
            AotStmt::Expr(expr) | AotStmt::ValueCarrier(expr) => collect_expr_reads(expr, out),
            AotStmt::Return(Some(expr)) => collect_expr_reads(expr, out),
            AotStmt::Return(None) | AotStmt::Break | AotStmt::Continue => {}
            AotStmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                collect_expr_reads(condition, out);
                collect_stmt_reads(then_branch, out);
                if let Some(else_branch) = else_branch {
                    collect_stmt_reads(else_branch, out);
                }
            }
            AotStmt::While { condition, body } => {
                collect_expr_reads(condition, out);
                collect_stmt_reads(body, out);
            }
            AotStmt::ForRange {
                start,
                stop,
                step,
                body,
                ..
            } => {
                collect_expr_reads(start, out);
                collect_expr_reads(stop, out);
                if let Some(step) = step {
                    collect_expr_reads(step, out);
                }
                collect_stmt_reads(body, out);
            }
            AotStmt::ForEach { iter, body, .. } => {
                collect_expr_reads(iter, out);
                collect_stmt_reads(body, out);
            }
        }
    }
}

/// Whether a struct field type lowers to a `Copy` Rust primitive, so the struct
/// can `#[derive(Copy)]` (Issue #5158). Conservative: container / nested-struct /
/// string fields are not treated as `Copy` even when they might be.
fn static_type_is_copy_primitive(ty: &StaticType) -> bool {
    matches!(
        ty,
        StaticType::I64
            | StaticType::I128
            | StaticType::I32
            | StaticType::I16
            | StaticType::I8
            | StaticType::U64
            | StaticType::U128
            | StaticType::U32
            | StaticType::U16
            | StaticType::U8
            | StaticType::F64
            | StaticType::F32
            | StaticType::F16
            | StaticType::Bool
            | StaticType::Char
    )
}

fn static_type_is_const_global_primitive(ty: &StaticType) -> bool {
    matches!(
        ty,
        StaticType::I64
            | StaticType::I128
            | StaticType::I32
            | StaticType::I16
            | StaticType::I8
            | StaticType::U64
            | StaticType::U128
            | StaticType::U32
            | StaticType::U16
            | StaticType::U8
            | StaticType::F64
            | StaticType::F32
            | StaticType::F16
            | StaticType::Bool
            | StaticType::Char
            | StaticType::Nothing
            | StaticType::Missing
    )
}

impl AotCodeGenerator {
    pub(super) fn emitted_function_name(&self, func: &AotFunction) -> String {
        if self.needs_dispatch(&func.name) {
            func.mangled_name()
        } else {
            AotFunction::sanitize_function_name(&func.name)
        }
    }

    /// Build the method table for multiple dispatch
    pub(super) fn build_method_table(&mut self, program: &AotProgram) {
        self.multidispatch_funcs.clear();
        self.method_table.clear();
        self.function_method_counts.clear();

        // Group functions by name
        let mut func_groups: HashMap<String, Vec<&AotFunction>> = HashMap::new();
        for func in &program.functions {
            func_groups.entry(func.name.clone()).or_default().push(func);
        }

        for (name, methods) in func_groups {
            self.function_method_counts
                .insert(name.clone(), methods.len());
            let mut seen_signatures = HashSet::new();
            let mut entries = Vec::new();
            for f in methods {
                let param_types: Vec<_> = f.params.iter().map(|(_, ty)| ty.clone()).collect();
                let key = (f.mangled_name(), param_types.clone());
                if seen_signatures.insert(key.clone()) {
                    entries.push((key.0, key.1, f.return_type.clone()));
                }
            }

            if entries.len() > 1 {
                self.multidispatch_funcs.insert(name.clone());
            }

            self.method_table.insert(name, entries);
        }
    }
    /// Check if a function requires multiple dispatch
    pub(super) fn needs_dispatch(&self, func_name: &str) -> bool {
        self.multidispatch_funcs.contains(func_name)
    }

    pub(super) fn should_resolve_static_call(&self, func_name: &str) -> bool {
        self.needs_dispatch(func_name)
            || self
                .function_method_counts
                .get(func_name)
                .is_some_and(|count| *count == 1)
    }

    /// Emit dispatcher functions for all multidispatch functions
    pub(super) fn emit_dispatchers(&mut self) -> AotResult<()> {
        // Clone to avoid borrow issues
        let multidispatch: Vec<_> = self.multidispatch_funcs.iter().cloned().collect();

        for func_name in multidispatch {
            if let Some(methods) = self.method_table.get(&func_name).cloned() {
                self.emit_dispatcher(&func_name, &methods)?;
                self.blank_line();
            }
        }
        Ok(())
    }

    fn is_c_symbol_name(name: &str) -> bool {
        let mut chars = name.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        (first == '_' || first.is_ascii_alphabetic())
            && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
    }

    /// Emit a dispatcher function for a multidispatch function
    fn emit_dispatcher(
        &mut self,
        func_name: &str,
        methods: &[(String, Vec<StaticType>, StaticType)],
    ) -> AotResult<()> {
        if methods.is_empty() {
            return Ok(());
        }

        // A name may carry methods of different arities — notably a
        // default-argument function and its lower-arity forwarding stubs
        // (Issue #7044). The dynamic dispatcher is a single fixed-arity Rust
        // function, so it serves the maximum arity and only includes arms whose
        // arity matches; lower-arity stubs are reached by static, arity-resolved
        // call sites. Without this filter a 2-/1-tuple arm under a 3-tuple
        // `match` is an E0308 type error.
        let param_count = methods.iter().map(|m| m.1.len()).max().unwrap_or(0);
        let methods: Vec<_> = methods
            .iter()
            .filter(|m| m.1.len() == param_count)
            .cloned()
            .collect();
        let methods = methods.as_slice();

        if self.config.emit_comments {
            self.write_line(&format!(
                "// Dispatcher for {} with {} methods",
                func_name,
                methods.len()
            ));
        }

        let dispatcher_name = AotFunction::sanitize_function_name(func_name);
        let params: Vec<_> = (0..param_count)
            .map(|idx| format!("arg{}: Value", idx))
            .collect();
        self.write_line(&format!(
            "pub fn {}({}) -> RuntimeResult<Value> {{",
            dispatcher_name,
            params.join(", ")
        ));
        self.indent();

        let match_expr = Self::dispatcher_match_expr(param_count);
        self.write_line(&format!("match {} {{", match_expr));
        self.indent();

        for arm in self.dispatcher_ambiguity_arms(func_name, methods) {
            self.write_line(&arm);
        }

        let mut supported_methods: Vec<_> = methods
            .iter()
            .filter_map(|(mangled_name, param_types, return_type)| {
                self.dispatcher_arm(mangled_name, param_types, return_type)
            })
            .collect();
        supported_methods.sort_by_key(|m| std::cmp::Reverse(m.specificity));

        for arm in supported_methods {
            self.write_line(&arm.code);
        }

        self.write_line(&Self::dispatcher_no_match_arm(func_name, param_count));
        self.dedent();
        self.write_line("}");
        self.dedent();
        self.write_line("}");

        Ok(())
    }

    fn dispatcher_match_expr(param_count: usize) -> String {
        match param_count {
            0 => "()".to_string(),
            1 => "(arg0,)".to_string(),
            _ => {
                let args: Vec<_> = (0..param_count).map(|idx| format!("arg{}", idx)).collect();
                format!("({})", args.join(", "))
            }
        }
    }

    fn dispatcher_no_match_arm(func_name: &str, param_count: usize) -> String {
        let bindings: Vec<_> = (0..param_count).map(|idx| format!("arg{}", idx)).collect();
        let pattern = match param_count {
            0 => "()".to_string(),
            1 => format!("({},)", bindings[0]),
            _ => format!("({})", bindings.join(", ")),
        };
        let type_names = bindings
            .iter()
            .map(|name| format!("{}.type_name()", name))
            .collect::<Vec<_>>()
            .join(", ");
        let placeholders = std::iter::repeat_n("{}", param_count)
            .collect::<Vec<_>>()
            .join(", ");
        if param_count == 0 {
            format!(
                "{} => Err(RuntimeError::method_error(\"{}()\")),",
                pattern, func_name
            )
        } else {
            format!(
                "{} => Err(RuntimeError::method_error(format!(\"{}({})\", {}))),",
                pattern, func_name, placeholders, type_names
            )
        }
    }

    fn dispatcher_arm(
        &self,
        mangled_name: &str,
        param_types: &[StaticType],
        return_type: &StaticType,
    ) -> Option<DispatcherArm> {
        if !Self::can_wrap_return_value(return_type) {
            return None;
        }

        let mut patterns = Vec::new();
        let mut call_args = Vec::new();
        let mut specificity = 0usize;
        for (idx, ty) in param_types.iter().enumerate() {
            let binding = format!("arg{}", idx);
            let (pattern, call_arg, is_specific) = Self::dispatcher_param_pattern(ty, &binding)?;
            if is_specific {
                specificity += 1;
            }
            patterns.push(pattern);
            call_args.push(call_arg);
        }

        let pattern = match patterns.len() {
            0 => "()".to_string(),
            1 => format!("({},)", patterns[0]),
            _ => format!("({})", patterns.join(", ")),
        };
        Some(DispatcherArm {
            specificity,
            code: format!(
                "{} => Ok(Value::from({}({}))),",
                pattern,
                mangled_name,
                call_args.join(", ")
            ),
        })
    }

    fn dispatcher_param_pattern(ty: &StaticType, binding: &str) -> Option<(String, String, bool)> {
        match ty {
            StaticType::I64 => Some((
                format!("Value::I64({})", binding),
                binding.to_string(),
                true,
            )),
            StaticType::I32 => Some((
                format!("Value::I32({})", binding),
                binding.to_string(),
                true,
            )),
            StaticType::F64 => Some((
                format!("Value::F64({})", binding),
                binding.to_string(),
                true,
            )),
            StaticType::F32 => Some((
                format!("Value::F32({})", binding),
                binding.to_string(),
                true,
            )),
            StaticType::Bool => Some((
                format!("Value::Bool({})", binding),
                binding.to_string(),
                true,
            )),
            StaticType::Char => Some((
                format!("Value::Char({})", binding),
                binding.to_string(),
                true,
            )),
            StaticType::Str => Some((
                format!("Value::Str({})", binding),
                binding.to_string(),
                true,
            )),
            StaticType::Nothing => Some(("Value::Nothing".to_string(), "()".to_string(), true)),
            StaticType::Any => Some((binding.to_string(), binding.to_string(), false)),
            StaticType::Union { variants } if variants.len() == 1 => {
                Self::dispatcher_param_pattern(&variants[0], binding)
            }
            _ => None,
        }
    }

    fn can_wrap_return_value(ty: &StaticType) -> bool {
        match ty {
            StaticType::I64
            | StaticType::I32
            | StaticType::F64
            | StaticType::F32
            | StaticType::Bool
            | StaticType::Char
            | StaticType::Str
            | StaticType::Nothing
            | StaticType::Any => true,
            StaticType::Array { element, .. } => Self::can_wrap_return_value(element),
            StaticType::Union { variants } if variants.len() == 1 => {
                Self::can_wrap_return_value(&variants[0])
            }
            _ => false,
        }
    }

    fn dispatcher_ambiguity_arms(
        &self,
        func_name: &str,
        methods: &[(String, Vec<StaticType>, StaticType)],
    ) -> Vec<String> {
        let mut arms = Vec::new();
        for (left_idx, (_, left_params, _)) in methods.iter().enumerate() {
            for (_, right_params, _) in methods.iter().skip(left_idx + 1) {
                if left_params.len() != right_params.len()
                    || Self::signature_more_specific(left_params, right_params)
                    || Self::signature_more_specific(right_params, left_params)
                {
                    continue;
                }

                let Some(intersection) = Self::method_intersection(left_params, right_params)
                else {
                    continue;
                };
                if methods.iter().any(|(_, candidate_params, _)| {
                    candidate_params != left_params
                        && candidate_params != right_params
                        && candidate_params.len() == intersection.len()
                        && Self::signature_more_specific(candidate_params, left_params)
                        && Self::signature_more_specific(candidate_params, right_params)
                        && candidate_params
                            .iter()
                            .zip(intersection.iter())
                            .all(|(param, arg)| self.types_match(param, arg))
                }) {
                    continue;
                }

                if let Some(pattern) = Self::dispatcher_ambiguity_pattern(&intersection) {
                    let signature = Self::method_signature(func_name, &intersection);
                    arms.push(format!(
                        "{} => Err(RuntimeError::method_error(\"{} is ambiguous\")),",
                        pattern, signature
                    ));
                }
            }
        }
        arms.sort();
        arms.dedup();
        arms
    }

    /// Resolve static dispatch for a function call.
    ///
    /// Given a function name and argument types, returns the mangled name of the
    /// unique most-specific method. If no method or more than one incomparable
    /// best method matches, return a Julia-shaped diagnostic instead of silently
    /// selecting an arbitrary method.
    pub(super) fn resolve_dispatch(
        &self,
        func_name: &str,
        arg_types: &[StaticType],
    ) -> AotResult<String> {
        if let Some(methods) = self.method_table.get(func_name) {
            let mut matches = Vec::new();
            for (idx, (mangled_name, param_types, _)) in methods.iter().enumerate() {
                if param_types.len() != arg_types.len() {
                    continue;
                }
                let params_match = param_types
                    .iter()
                    .zip(arg_types.iter())
                    .all(|(param, arg)| self.types_match(param, arg));
                if !params_match {
                    continue;
                }
                matches.push((idx, mangled_name, param_types));
            }

            let mut best = Vec::new();
            'candidate: for candidate in &matches {
                for other in &matches {
                    if candidate.0 != other.0
                        && Self::method_more_specific(other.2, candidate.2, arg_types)
                    {
                        continue 'candidate;
                    }
                }
                best.push(*candidate);
            }

            if best.len() == 1 {
                return Ok(if self.needs_dispatch(func_name) {
                    best[0].1.clone()
                } else {
                    AotFunction::sanitize_function_name(func_name)
                });
            }

            let signature = Self::method_signature(func_name, arg_types);
            if best.len() > 1 {
                return Err(AotError::CodegenError(format!(
                    "{} is ambiguous",
                    signature
                )));
            }

            return Err(AotError::CodegenError(format!(
                "no method matching {}",
                signature
            )));
        }

        // No dispatch needed or no methods found
        Ok(AotFunction::sanitize_function_name(func_name))
    }

    /// Check if two types match for dispatch resolution
    fn types_match(&self, expected: &StaticType, actual: &StaticType) -> bool {
        // Exact match
        if expected == actual {
            return true;
        }

        // A Julia `Any` parameter is a fallback method and accepts concrete
        // arguments. An `Any` actual argument is not concrete enough to pick a
        // non-Any method statically.
        if matches!(expected, StaticType::Any) {
            return true;
        }

        if Self::bare_complex_accepts_concrete(expected, actual) {
            return true;
        }

        false
    }

    fn bare_complex_accepts_concrete(expected: &StaticType, actual: &StaticType) -> bool {
        matches!(
            (expected, actual),
            (
                StaticType::Struct {
                    name: expected_name,
                    ..
                },
                StaticType::Struct {
                    name: actual_name, ..
                }
            ) if Self::is_bare_complex_name(expected_name)
                && StaticType::complex_param_type_from_name(actual_name).is_some()
        )
    }

    fn is_bare_complex_name(name: &str) -> bool {
        name == "Complex"
    }

    fn method_more_specific(
        lhs_params: &[StaticType],
        rhs_params: &[StaticType],
        arg_types: &[StaticType],
    ) -> bool {
        if lhs_params.len() != rhs_params.len() || lhs_params.len() != arg_types.len() {
            return false;
        }

        let mut strictly_more_specific = false;
        for ((lhs, rhs), arg) in lhs_params
            .iter()
            .zip(rhs_params.iter())
            .zip(arg_types.iter())
        {
            let lhs_score = Self::param_specificity(lhs, arg);
            let rhs_score = Self::param_specificity(rhs, arg);
            if lhs_score < rhs_score {
                return false;
            }
            if lhs_score > rhs_score {
                strictly_more_specific = true;
            }
        }
        strictly_more_specific
    }

    fn param_specificity(param: &StaticType, arg: &StaticType) -> u8 {
        if param == arg {
            2
        } else if matches!(param, StaticType::Any) {
            0
        } else {
            1
        }
    }

    fn signature_more_specific(lhs_params: &[StaticType], rhs_params: &[StaticType]) -> bool {
        if lhs_params.len() != rhs_params.len() {
            return false;
        }

        let mut strictly_more_specific = false;
        for (lhs, rhs) in lhs_params.iter().zip(rhs_params.iter()) {
            if lhs == rhs {
                continue;
            }
            if Self::bare_complex_accepts_concrete(rhs, lhs) {
                strictly_more_specific = true;
                continue;
            }
            if matches!(rhs, StaticType::Any) && !matches!(lhs, StaticType::Any) {
                strictly_more_specific = true;
                continue;
            }
            return false;
        }
        strictly_more_specific
    }

    fn method_intersection(lhs: &[StaticType], rhs: &[StaticType]) -> Option<Vec<StaticType>> {
        if lhs.len() != rhs.len() {
            return None;
        }

        lhs.iter()
            .zip(rhs.iter())
            .map(|(left, right)| {
                if left == right {
                    Some(left.clone())
                } else if matches!(left, StaticType::Any) {
                    Some(right.clone())
                } else if matches!(right, StaticType::Any) {
                    Some(left.clone())
                } else if Self::bare_complex_accepts_concrete(left, right) {
                    Some(right.clone())
                } else if Self::bare_complex_accepts_concrete(right, left) {
                    Some(left.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    fn dispatcher_ambiguity_pattern(types: &[StaticType]) -> Option<String> {
        let patterns: Option<Vec<_>> = types
            .iter()
            .map(|ty| Self::dispatcher_param_pattern(ty, "_").map(|(pattern, _, _)| pattern))
            .collect();
        let patterns = patterns?;
        Some(match patterns.len() {
            0 => "()".to_string(),
            1 => format!("({},)", patterns[0]),
            _ => format!("({})", patterns.join(", ")),
        })
    }

    fn method_signature(func_name: &str, arg_types: &[StaticType]) -> String {
        if arg_types.is_empty() {
            return format!("{}()", func_name);
        }
        let args = arg_types
            .iter()
            .map(|ty| format!("::{}", ty.julia_type_name()))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{}({})", func_name, args)
    }

    /// Check if a string represents a closure literal
    ///
    /// Closures start with `|` or `move |` in Rust syntax.
    pub(super) fn is_closure_literal(s: &str) -> bool {
        let trimmed = s.trim();
        trimmed.starts_with('|') || trimmed.starts_with("move |")
    }

    /// Emit prelude (imports and setup)
    pub(super) fn emit_prelude(&mut self) {
        self.write_line("//! Auto-generated by SubsetJuliaVM AoT compiler");
        self.write_line("//! Do not edit manually");
        self.blank_line();
        self.write_line("#![allow(unused_variables)]");
        self.write_line("#![allow(unused_mut)]");
        self.write_line("#![allow(unused_imports)]");
        self.write_line("#![allow(unused_must_use)]");
        self.write_line("#![allow(dead_code)]");
        self.write_line("#![allow(non_upper_case_globals)]");
        self.write_line("#![allow(non_snake_case)]");
        // The binary-op emitter wraps every operation in parentheses to preserve
        // precedence in nested positions; at the top level of a statement (and as a
        // sole function argument) these are redundant and trip rustc's
        // `unused_parens` and clippy's `double_parens` lints (Issue #7311). The
        // parens are harmless and keeping the emitter context-free is far safer than
        // making it precedence-aware, so we silence the lints here. `unused_braces`
        // is allowed for the same reason.
        self.write_line("#![allow(unused_parens)]");
        self.write_line("#![allow(unused_braces)]");
        self.write_line("#![allow(clippy::double_parens)]");
        self.write_line("#![allow(clippy::needless_range_loop)]");
        self.write_line("#![allow(clippy::no_effect)]");
        self.blank_line();
        // Import the dynamic Value type selected by the AoT ABI boundary for
        // unknown or multi-variant Union values.
        self.write_line("extern crate subset_julia_vm_runtime;");
        self.write_line(&format!(
            "const _: [(); subset_julia_vm_runtime::AOT_RUNTIME_ABI_VERSION] = [(); {}];",
            AOT_RUNTIME_ABI_VERSION
        ));
        self.write_line("use subset_julia_vm_runtime::{RuntimeError, RuntimeResult, Value};");
        self.blank_line();

        self.write_line("thread_local! {");
        self.indent();
        self.write_line("static __SJULIA_AOT_RNG: std::cell::RefCell<subset_julia_vm_runtime::rng::StableRng> = std::cell::RefCell::new(subset_julia_vm_runtime::rng::StableRng::new(42));");
        self.dedent();
        self.write_line("}");
        self.write_line("#[inline]");
        self.write_line("fn __sjulia_aot_rand() -> f64 {");
        self.indent();
        self.write_line("__SJULIA_AOT_RNG.with(|rng| subset_julia_vm_runtime::rng::RngLike::next_f64(&mut *rng.borrow_mut()))");
        self.dedent();
        self.write_line("}");
        self.write_line("#[inline]");
        self.write_line("fn __sjulia_aot_randn() -> f64 {");
        self.indent();
        self.write_line("__SJULIA_AOT_RNG.with(|rng| subset_julia_vm_runtime::rng::randn(&mut *rng.borrow_mut()))");
        self.dedent();
        self.write_line("}");
        self.blank_line();

        // AoT broadcast helpers used by ir_converter broadcast lowering.
        self.write_line("fn __aot_broadcast_mul_scalar_vec<F, S: Clone, T: Clone, R>(f: F, scalar: S, values: Vec<T>) -> Vec<R>");
        self.write_line("where");
        self.indent();
        self.write_line("F: Fn(S, T) -> R + Copy,");
        self.dedent();
        self.write_line("{");
        self.indent();
        self.write_line("let mut out: Vec<R> = Vec::with_capacity(values.len());");
        self.write_line("for value in values {");
        self.indent();
        self.write_line("out.push(f(scalar.clone(), value.clone()));");
        self.dedent();
        self.write_line("}");
        self.write_line("out");
        self.dedent();
        self.write_line("}");
        self.blank_line();

        self.write_line("fn __aot_broadcast_add_row_vec<F, A: Clone, B: Clone, R>(f: F, row: Vec<Vec<A>>, col: Vec<B>) -> Vec<Vec<R>>");
        self.write_line("where");
        self.indent();
        self.write_line("F: Fn(A, B) -> R + Copy,");
        self.dedent();
        self.write_line("{");
        self.indent();
        self.write_line("let width = if row.is_empty() { 0 } else { row[0].len() };");
        self.write_line("let mut out: Vec<Vec<R>> = Vec::with_capacity(col.len());");
        self.write_line("for c in col {");
        self.indent();
        self.write_line("let mut out_row: Vec<R> = Vec::with_capacity(width);");
        self.write_line("for i in 0..width {");
        self.indent();
        self.write_line("out_row.push(f(row[0][i].clone(), c.clone()));");
        self.dedent();
        self.write_line("}");
        self.write_line("out.push(out_row);");
        self.dedent();
        self.write_line("}");
        self.write_line("out");
        self.dedent();
        self.write_line("}");
        self.blank_line();

        self.write_line("fn __aot_broadcast_call_matrix_scalar_2<F, T: Clone, U: Clone, R>(f: F, matrix: Vec<Vec<T>>, scalar: U) -> Vec<Vec<R>>");
        self.write_line("where");
        self.indent();
        self.write_line("F: Fn(T, U) -> R + Copy,");
        self.dedent();
        self.write_line("{");
        self.indent();
        self.write_line("let mut out: Vec<Vec<R>> = Vec::with_capacity(matrix.len());");
        self.write_line("for row in matrix {");
        self.indent();
        self.write_line("let mut out_row: Vec<R> = Vec::with_capacity(row.len());");
        self.write_line("for value in row {");
        self.indent();
        self.write_line("out_row.push(f(value.clone(), scalar.clone()));");
        self.dedent();
        self.write_line("}");
        self.write_line("out.push(out_row);");
        self.dedent();
        self.write_line("}");
        self.write_line("out");
        self.dedent();
        self.write_line("}");
        self.blank_line();

        // ErrorException struct and throw function for Julia error handling (Issue #3406).
        // Julia's throw(ErrorException(msg)) maps to the runtime's diverging
        // `aot_throw`, so generated files stay free of raw `panic!` (Issue #5658)
        // while preserving the abort-on-throw semantics.
        self.write_line("#[derive(Debug)]");
        self.write_line("struct ErrorException { msg: String }");
        self.write_line("impl ErrorException {");
        self.indent();
        self.write_line("fn new(s: String) -> Self { ErrorException { msg: s } }");
        self.dedent();
        self.write_line("}");
        self.blank_line();
        self.write_line("impl std::fmt::Display for ErrorException {");
        self.indent();
        self.write_line("fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {");
        self.indent();
        self.write_line("write!(f, \"{}\", self.msg)");
        self.dedent();
        self.write_line("}");
        self.dedent();
        self.write_line("}");
        self.blank_line();
        self.write_line(
            "fn throw<T: std::fmt::Display>(e: T) -> ! { subset_julia_vm_runtime::error::aot_throw(e) }",
        );
        self.blank_line();

        // Julia's print/display path keeps a decimal point for whole Float64/Float32
        // values (`3.0`, `-0.0`), uses `Inf` spelling rather than Rust's `inf`
        // (Issue #7013), and switches large/small magnitudes to scientific
        // notation (`1.0e30`, not `1000000000000000000000000000000`; Issue #7256).
        // The canonical algorithm lives in the runtime crate
        // (`subset_julia_vm_runtime::intrinsics::format_float64_julia`) so AoT
        // output stays in lock-step with the VM-side `format_float_julia` and
        // upstream Julia; emit thin wrappers that delegate to it. Keeping this at
        // the I/O/string boundary avoids boxing static floats into runtime Value.
        self.write_line("fn __sjulia_format_float64(value: f64) -> String {");
        self.indent();
        self.write_line("subset_julia_vm_runtime::intrinsics::format_float64_julia(value)");
        self.dedent();
        self.write_line("}");
        self.blank_line();
        self.write_line("fn __sjulia_format_float32(value: f32) -> String {");
        self.indent();
        self.write_line("subset_julia_vm_runtime::intrinsics::format_float32_julia(value)");
        self.dedent();
        self.write_line("}");
        self.blank_line();

        // linspace: linearly spaced vector (replacement for range(start,stop;length=n)) (Issue #3413)
        self.write_line("fn linspace(start: f64, stop: f64, n: i64) -> Vec<f64> {");
        self.indent();
        self.write_line("if n <= 0 { return vec![]; }");
        self.write_line("if n == 1 { return vec![start]; }");
        self.write_line("let step = (stop - start) / ((n - 1) as f64);");
        self.write_line("(0..n).map(|i| start + (i as f64) * step).collect()");
        self.dedent();
        self.write_line("}");
        self.blank_line();

        self.write_line("trait SjuliaRangeElement: Copy + PartialEq + PartialOrd {");
        self.indent();
        self.write_line("fn zero() -> Self;");
        self.write_line("fn checked_add_step(self, step: Self) -> Option<Self>;");
        self.dedent();
        self.write_line("}");
        self.blank_line();
        self.write_line("#[derive(Clone, Debug)]");
        self.write_line("struct SjuliaRange<T: SjuliaRangeElement> { start: T, stop: T, step: T }");
        self.write_line("impl<T: SjuliaRangeElement> SjuliaRange<T> {");
        self.indent();
        self.write_line("fn new(start: T, stop: T, step: T) -> Self {");
        self.indent();
        self.write_line("if step == T::zero() { subset_julia_vm_runtime::error::aot_throw(\"ArgumentError: step cannot be zero\"); }");
        self.write_line("SjuliaRange { start, stop, step }");
        self.dedent();
        self.write_line("}");
        self.write_line("fn len(&self) -> usize { self.clone().into_iter().count() }");
        self.dedent();
        self.write_line("}");
        self.write_line("struct SjuliaRangeIter<T: SjuliaRangeElement> { current: T, stop: T, step: T, done: bool }");
        self.write_line("impl<T: SjuliaRangeElement> Iterator for SjuliaRangeIter<T> {");
        self.indent();
        self.write_line("type Item = T;");
        self.write_line("fn next(&mut self) -> Option<T> {");
        self.indent();
        self.write_line("if self.done { return None; }");
        self.write_line("let forward = self.step > T::zero();");
        self.write_line("if (forward && self.current > self.stop) || (!forward && self.current < self.stop) { self.done = true; return None; }");
        self.write_line("let out = self.current;");
        self.write_line("match self.current.checked_add_step(self.step) {");
        self.indent();
        self.write_line("Some(next) if (forward && next > self.current) || (!forward && next < self.current) => self.current = next,");
        self.write_line("_ => self.done = true,");
        self.dedent();
        self.write_line("}");
        self.write_line("Some(out)");
        self.dedent();
        self.write_line("}");
        self.dedent();
        self.write_line("}");
        self.write_line("impl<T: SjuliaRangeElement> IntoIterator for SjuliaRange<T> {");
        self.indent();
        self.write_line("type Item = T;");
        self.write_line("type IntoIter = SjuliaRangeIter<T>;");
        self.write_line("fn into_iter(self) -> Self::IntoIter { SjuliaRangeIter { current: self.start, stop: self.stop, step: self.step, done: false } }");
        self.dedent();
        self.write_line("}");
        self.write_line("macro_rules! sjulia_range_int { ($($t:ty),* $(,)?) => { $(impl SjuliaRangeElement for $t { fn zero() -> Self { 0 as $t } fn checked_add_step(self, step: Self) -> Option<Self> { self.checked_add(step) } })* }; }");
        self.write_line("sjulia_range_int!(i8, i16, i32, i64, i128, u8, u16, u32, u64, u128);");
        self.write_line("impl SjuliaRangeElement for f32 { fn zero() -> Self { 0.0 } fn checked_add_step(self, step: Self) -> Option<Self> { Some(self + step) } }");
        self.write_line("impl SjuliaRangeElement for f64 { fn zero() -> Self { 0.0 } fn checked_add_step(self, step: Self) -> Option<Self> { Some(self + step) } }");
        self.blank_line();
        self.write_line("#[derive(Clone, Debug)]");
        self.write_line("struct SjuliaCharRange { start: char, stop: char }");
        self.write_line("impl SjuliaCharRange {");
        self.indent();
        self.write_line(
            "fn new(start: char, stop: char) -> Self { SjuliaCharRange { start, stop } }",
        );
        self.write_line("fn len(&self) -> usize { self.clone().into_iter().count() }");
        self.dedent();
        self.write_line("}");
        self.write_line("struct SjuliaCharRangeIter { current: u32, stop: u32, done: bool }");
        self.write_line("impl Iterator for SjuliaCharRangeIter {");
        self.indent();
        self.write_line("type Item = char;");
        self.write_line("fn next(&mut self) -> Option<char> {");
        self.indent();
        self.write_line(
            "if self.done || self.current > self.stop { self.done = true; return None; }",
        );
        self.write_line("let code = self.current;");
        self.write_line("self.current = self.current.checked_add(1).unwrap_or_else(|| { self.done = true; self.current });");
        self.write_line("Some(std::char::from_u32(code).unwrap_or_else(|| subset_julia_vm_runtime::error::aot_throw(format!(\"Char({}) is not a valid Rust char\", code))))");
        self.dedent();
        self.write_line("}");
        self.dedent();
        self.write_line("}");
        self.write_line("impl IntoIterator for SjuliaCharRange {");
        self.indent();
        self.write_line("type Item = char;");
        self.write_line("type IntoIter = SjuliaCharRangeIter;");
        self.write_line("fn into_iter(self) -> Self::IntoIter { SjuliaCharRangeIter { current: self.start as u32, stop: self.stop as u32, done: false } }");
        self.dedent();
        self.write_line("}");
        self.blank_line();

        // Operator function wrappers used by broadcast helpers
        self.write_line("fn op_add(a: f64, b: f64) -> f64 { a + b }");
        self.write_line("fn op_sub(a: f64, b: f64) -> f64 { a - b }");
        self.write_line("fn op_mul(a: f64, b: f64) -> f64 { a * b }");
        self.write_line("fn op_div(a: f64, b: f64) -> f64 { a / b }");
        self.blank_line();

        // Julia-faithful bitwise NOT and shift semantics (Issue #7057).
        // Julia clamps the shift amount: shifting by >= the bit width yields 0
        // (or sign fill for arithmetic `>>` on signed values), and a negative
        // amount shifts the other direction. Rust's native `<<`/`>>` instead
        // panic (debug) or mask the amount, so route shifts through helpers.
        self.write_line("fn op_bnot<T: std::ops::Not<Output = T>>(x: T) -> T { !x }");
        self.write_line("trait SjuliaShift: Copy {");
        self.write_line("    fn sjulia_shl(self, k: i64) -> Self;");
        self.write_line("    fn sjulia_ashr(self, k: i64) -> Self;");
        self.write_line("    fn sjulia_lshr(self, k: i64) -> Self;");
        self.write_line("}");
        self.write_line("macro_rules! sjulia_shift_signed { ($t:ty, $u:ty, $bits:expr) => {");
        self.write_line("    impl SjuliaShift for $t {");
        self.write_line("        fn sjulia_shl(self, k: i64) -> Self {");
        self.write_line(
            "            if k < 0 { return self.sjulia_ashr(k.unsigned_abs().min($bits) as i64); }",
        );
        self.write_line(
            "            if k >= $bits { 0 } else { (self as $u).wrapping_shl(k as u32) as $t }",
        );
        self.write_line("        }");
        self.write_line("        fn sjulia_ashr(self, k: i64) -> Self {");
        self.write_line(
            "            if k < 0 { return self.sjulia_shl(k.unsigned_abs().min($bits) as i64); }",
        );
        self.write_line("            if k >= $bits { if self < 0 { -1 } else { 0 } } else { self >> (k as u32) }");
        self.write_line("        }");
        self.write_line("        fn sjulia_lshr(self, k: i64) -> Self {");
        self.write_line(
            "            if k < 0 { return self.sjulia_shl(k.unsigned_abs().min($bits) as i64); }",
        );
        self.write_line(
            "            if k >= $bits { 0 } else { ((self as $u) >> (k as u32)) as $t }",
        );
        self.write_line("        }");
        self.write_line("    }");
        self.write_line("}; }");
        self.write_line("macro_rules! sjulia_shift_unsigned { ($t:ty, $bits:expr) => {");
        self.write_line("    impl SjuliaShift for $t {");
        self.write_line("        fn sjulia_shl(self, k: i64) -> Self {");
        self.write_line(
            "            if k < 0 { return self.sjulia_lshr(k.unsigned_abs().min($bits) as i64); }",
        );
        self.write_line("            if k >= $bits { 0 } else { self.wrapping_shl(k as u32) }");
        self.write_line("        }");
        self.write_line("        fn sjulia_ashr(self, k: i64) -> Self { self.sjulia_lshr(k) }");
        self.write_line("        fn sjulia_lshr(self, k: i64) -> Self {");
        self.write_line(
            "            if k < 0 { return self.sjulia_shl(k.unsigned_abs().min($bits) as i64); }",
        );
        self.write_line("            if k >= $bits { 0 } else { self.wrapping_shr(k as u32) }");
        self.write_line("        }");
        self.write_line("    }");
        self.write_line("}; }");
        self.write_line("sjulia_shift_signed!(i8, u8, 8);");
        self.write_line("sjulia_shift_signed!(i16, u16, 16);");
        self.write_line("sjulia_shift_signed!(i32, u32, 32);");
        self.write_line("sjulia_shift_signed!(i64, u64, 64);");
        self.write_line("sjulia_shift_signed!(i128, u128, 128);");
        self.write_line("sjulia_shift_unsigned!(u8, 8);");
        self.write_line("sjulia_shift_unsigned!(u16, 16);");
        self.write_line("sjulia_shift_unsigned!(u32, 32);");
        self.write_line("sjulia_shift_unsigned!(u64, 64);");
        self.write_line("sjulia_shift_unsigned!(u128, 128);");
        self.write_line("fn op_lshift<T: SjuliaShift>(x: T, k: i64) -> T { x.sjulia_shl(k) }");
        self.write_line("fn op_rshift<T: SjuliaShift>(x: T, k: i64) -> T { x.sjulia_ashr(k) }");
        self.write_line("fn op_urshift<T: SjuliaShift>(x: T, k: i64) -> T { x.sjulia_lshr(k) }");
        self.blank_line();

        // Broadcast helper for 1D + 1D outer product: row ⊕ col → 2D matrix (Issue #3410).
        self.write_line("fn __aot_broadcast_outer_product<F, A: Clone, B: Clone, R>(f: F, row: Vec<A>, col: Vec<B>) -> Vec<Vec<R>>");
        self.write_line("where");
        self.indent();
        self.write_line("F: Fn(A, B) -> R + Copy,");
        self.dedent();
        self.write_line("{");
        self.indent();
        self.write_line("col.iter().map(|c| {");
        self.indent();
        self.write_line("row.iter().map(|r| f(r.clone(), c.clone())).collect()");
        self.dedent();
        self.write_line("}).collect()");
        self.dedent();
        self.write_line("}");
        self.blank_line();
    }

    /// Emit prelude stubs that depend on struct definitions (emitted after structs).
    /// These reference Complex and other user-defined types (Issue #3410).
    pub(super) fn emit_struct_dependent_prelude(&mut self, has_complex: bool) {
        if !has_complex {
            return;
        }
        self.blank_line();

        // Julia's global imaginary unit. Keep the Rust spelling lowercase so
        // user/local bindings named `im` shadow it with normal lexical scoping
        // instead of going through an internal alias (Issue #6966).
        self.write_line("const im: Complex = Complex::<f64> { re: 0.0, im: 1.0 };");
        self.blank_line();

        self.write_line("impl<T> From<Complex<T>> for Value");
        self.write_line("where T: Into<Value>");
        self.write_line("{");
        self.indent();
        self.write_line("fn from(value: Complex<T>) -> Self {");
        self.indent();
        self.write_line("Value::Struct {");
        self.indent();
        self.write_line("type_name: \"Complex\".to_string(),");
        self.write_line("fields: vec![value.re.into(), value.im.into()],");
        self.dedent();
        self.write_line("}");
        self.dedent();
        self.write_line("}");
        self.dedent();
        self.write_line("}");
        self.blank_line();

        // Mixed-type operator impls for Float64-backed Complex arithmetic.
        self.write_line("impl std::ops::Mul<Complex> for f64 {");
        self.indent();
        self.write_line("type Output = Complex;");
        self.write_line(
            "fn mul(self, rhs: Complex) -> Complex { Complex::new(self * rhs.re, self * rhs.im) }",
        );
        self.dedent();
        self.write_line("}");
        self.blank_line();

        self.write_line("impl std::ops::Mul<Complex> for i64 {");
        self.indent();
        self.write_line("type Output = Complex;");
        self.write_line("fn mul(self, rhs: Complex) -> Complex { Complex::new((self as f64) * rhs.re, (self as f64) * rhs.im) }");
        self.dedent();
        self.write_line("}");
        self.blank_line();

        self.write_line("impl std::ops::Add<Complex> for f64 {");
        self.indent();
        self.write_line("type Output = Complex;");
        self.write_line(
            "fn add(self, rhs: Complex) -> Complex { Complex::new(self + rhs.re, rhs.im) }",
        );
        self.dedent();
        self.write_line("}");
        self.blank_line();

        self.write_line("impl std::ops::Add<f64> for Complex {");
        self.indent();
        self.write_line("type Output = Complex;");
        self.write_line(
            "fn add(self, rhs: f64) -> Complex { Complex::new(self.re + rhs, self.im) }",
        );
        self.dedent();
        self.write_line("}");
        self.blank_line();

        self.write_line("impl std::ops::Add<i64> for Complex {");
        self.indent();
        self.write_line("type Output = Complex;");
        self.write_line(
            "fn add(self, rhs: i64) -> Complex { Complex::new(self.re + (rhs as f64), self.im) }",
        );
        self.dedent();
        self.write_line("}");
        self.blank_line();

        // abs2 for Complex numbers: |z|^2 = re^2 + im^2
        self.write_line("fn abs2_complex<T>(z: Complex<T>) -> T");
        self.write_line("where T: Copy + std::ops::Add<Output = T> + std::ops::Mul<Output = T>");
        self.write_line("{ z.re * z.re + z.im * z.im }");
        self.write_line("fn abs2_f64(x: f64) -> f64 { x * x }");
        self.write_line("fn __sjulia_value_as_complex(value: &Value) -> Option<Complex> {");
        self.indent();
        self.write_line("let Value::Struct { type_name, fields } = value else { return None; };");
        self.write_line(
            "if type_name != \"Complex\" && !type_name.starts_with(\"Complex{\") { return None; }",
        );
        self.write_line("let [re, imag] = fields.as_slice() else { return None; };");
        self.write_line("Some(Complex::new(re.as_f64()?, imag.as_f64()?))");
        self.dedent();
        self.write_line("}");
        self.write_line("fn abs2_value(value: &Value) -> f64 {");
        self.indent();
        self.write_line("if let Some(x) = value.as_f64() { return x * x; }");
        self.write_line("let z = __sjulia_value_as_complex(value).unwrap_or_else(|| throw(RuntimeError::method_error(format!(\"abs2({}::{})\", value, value.type_name()))));");
        self.write_line("abs2_complex(z)");
        self.dedent();
        self.write_line("}");
        self.blank_line();

        // real/imag for Complex numbers
        self.write_line("fn real_complex<T: Copy>(z: Complex<T>) -> T { z.re }");
        self.write_line("fn imag_complex<T: Copy>(z: Complex<T>) -> T { z.im }");
        self.blank_line();

        // adjoint: identity for 1D vectors
        self.write_line("fn adjoint_vec(x: Vec<f64>) -> Vec<f64> { x }");
        self.blank_line();

        // Complex operator wrappers for broadcast (only those not already emitted by emit_struct)
        self.write_line("fn op_add_complex_complex(a: Complex, b: Complex) -> Complex { a + b }");
        self.write_line("fn op_mul_complex_i64(a: Complex, b: i64) -> Complex { Complex::new(a.re * (b as f64), a.im * (b as f64)) }");
        self.blank_line();
    }

    /// Emit a struct definition
    pub(super) fn emit_struct(&mut self, s: &AotStruct) -> AotResult<()> {
        if self.config.emit_comments {
            self.write_line(&format!("// Julia struct: {}", s.name));
        }

        if s.name == "Complex" {
            self.emit_complex_struct();
            return Ok(());
        }

        // Derive common traits. Issue #5158: `Copy` is derived structurally for
        // any immutable "isbits" struct (all fields lower to `Copy` primitives)
        // rather than special-casing the name `"Complex"`. `Copy` is a strict
        // superset of `Clone`, so code that compiled with `Clone` still compiles.
        let is_copy = !s.is_mutable
            && !s.fields.is_empty()
            && s.fields.iter().all(|(_, ty)| {
                static_type_is_copy_primitive(ty)
                    || matches!(ty, StaticType::Struct { name, .. } if s.type_params.iter().any(|param| param == name))
            });
        if is_copy {
            self.write_line("#[derive(Debug, Clone, Copy)]");
        } else {
            self.write_line("#[derive(Debug, Clone)]");
        }

        // Struct definition
        let generic_params = if s.type_params.is_empty() {
            String::new()
        } else {
            format!("<{}>", s.type_params.join(", "))
        };
        self.write_line(&format!("pub struct {}{} {{", s.name, generic_params));
        self.indent();

        for (field_name, field_ty) in &s.fields {
            let rust_ty = self.type_to_rust(field_ty);
            let escaped = escape_rust_ident(field_name);
            self.write_line(&format!("pub {}: {},", escaped, rust_ty));
        }

        self.dedent();
        self.write_line("}");

        // Constructor impl
        self.blank_line();
        self.write_line(&format!(
            "impl{} {}{} {{",
            generic_params, s.name, generic_params
        ));
        self.indent();

        // new() constructor
        let params: Vec<_> = s
            .fields
            .iter()
            .map(|(name, ty)| {
                format!(
                    "__sjulia_field_{}: {}",
                    escape_rust_ident(name),
                    self.type_to_rust(ty)
                )
            })
            .collect();
        self.write_line(&format!("pub fn new({}) -> Self {{", params.join(", ")));
        self.indent();
        self.write_line("Self {");
        self.indent();
        for (field_name, _) in &s.fields {
            let escaped = escape_rust_ident(field_name);
            self.write_line(&format!("{}: __sjulia_field_{},", escaped, escaped));
        }
        self.dedent();
        self.write_line("}");
        self.dedent();
        self.write_line("}");

        self.dedent();
        self.write_line("}");

        Ok(())
    }

    pub(super) fn emit_complex_struct(&mut self) {
        self.write_line("#[derive(Debug, Clone, Copy)]");
        self.write_line("pub struct Complex<T = f64> {");
        self.indent();
        self.write_line("pub re: T,");
        self.write_line("pub im: T,");
        self.dedent();
        self.write_line("}");
        self.blank_line();

        self.write_line("impl<T> Complex<T> {");
        self.indent();
        self.write_line("pub fn new(__sjulia_field_re: T, __sjulia_field_im: T) -> Self {");
        self.indent();
        self.write_line("Self { re: __sjulia_field_re, im: __sjulia_field_im }");
        self.dedent();
        self.write_line("}");
        self.dedent();
        self.write_line("}");
        self.blank_line();

        self.write_line("impl<T> std::ops::Add for Complex<T>");
        self.write_line("where T: std::ops::Add<Output = T>");
        self.write_line("{");
        self.indent();
        self.write_line("type Output = Complex<T>;");
        self.write_line("fn add(self, rhs: Complex<T>) -> Self::Output {");
        self.indent();
        self.write_line("Complex::new(self.re + rhs.re, self.im + rhs.im)");
        self.dedent();
        self.write_line("}");
        self.dedent();
        self.write_line("}");
        self.blank_line();

        self.write_line("impl<T> std::ops::Sub for Complex<T>");
        self.write_line("where T: std::ops::Sub<Output = T>");
        self.write_line("{");
        self.indent();
        self.write_line("type Output = Complex<T>;");
        self.write_line("fn sub(self, rhs: Complex<T>) -> Self::Output {");
        self.indent();
        self.write_line("Complex::new(self.re - rhs.re, self.im - rhs.im)");
        self.dedent();
        self.write_line("}");
        self.dedent();
        self.write_line("}");
        self.blank_line();

        self.write_line("impl<T> std::ops::Mul for Complex<T>");
        self.write_line(
            "where T: Copy + std::ops::Add<Output = T> + std::ops::Sub<Output = T> + std::ops::Mul<Output = T>",
        );
        self.write_line("{");
        self.indent();
        self.write_line("type Output = Complex<T>;");
        self.write_line("fn mul(self, rhs: Complex<T>) -> Self::Output {");
        self.indent();
        self.write_line("Complex::new(self.re * rhs.re - self.im * rhs.im, self.re * rhs.im + self.im * rhs.re)");
        self.dedent();
        self.write_line("}");
        self.dedent();
        self.write_line("}");
        self.blank_line();

        self.write_line("fn op_add_f64_complex(x: f64, y: Complex) -> Complex {");
        self.indent();
        self.write_line("Complex::new(x + y.re, y.im)");
        self.dedent();
        self.write_line("}");
        self.blank_line();

        self.write_line("fn op_mul_complex_f64(x: Complex, y: f64) -> Complex {");
        self.indent();
        self.write_line("Complex::new(x.re * y, x.im * y)");
        self.dedent();
        self.write_line("}");
    }

    pub(super) fn ordered_structs_by_dependency(
        structs: &[AotStruct],
    ) -> AotResult<Vec<&AotStruct>> {
        let mut name_to_index = HashMap::new();
        for (idx, s) in structs.iter().enumerate() {
            if name_to_index.insert(s.name.as_str(), idx).is_some() {
                return Err(AotError::CodegenError(format!(
                    "AoT codegen cannot emit duplicate struct definition `{}` (Issue #6974)",
                    s.name
                )));
            }
        }

        let mut state = vec![0u8; structs.len()];
        let mut ordered = Vec::with_capacity(structs.len());
        for idx in 0..structs.len() {
            Self::visit_struct_dependency(idx, structs, &name_to_index, &mut state, &mut ordered)?;
        }
        Ok(ordered)
    }

    fn visit_struct_dependency<'a>(
        idx: usize,
        structs: &'a [AotStruct],
        name_to_index: &HashMap<&str, usize>,
        state: &mut [u8],
        ordered: &mut Vec<&'a AotStruct>,
    ) -> AotResult<()> {
        match state[idx] {
            2 => return Ok(()),
            1 => {
                return Err(AotError::CodegenError(format!(
                    "AoT codegen cannot emit cyclic struct dependency involving `{}` \
                     (Issue #6974)",
                    structs[idx].name
                )))
            }
            _ => {}
        }

        state[idx] = 1;
        let mut deps = Vec::new();
        for (_, field_ty) in &structs[idx].fields {
            Self::collect_struct_type_dependencies(field_ty, &mut deps);
        }
        let mut seen = HashSet::new();
        for dep in deps {
            if !seen.insert(dep.clone()) {
                continue;
            }
            let Some(dep_idx) = name_to_index.get(dep.as_str()) else {
                continue;
            };
            Self::visit_struct_dependency(*dep_idx, structs, name_to_index, state, ordered)?;
        }

        state[idx] = 2;
        ordered.push(&structs[idx]);
        Ok(())
    }

    fn collect_struct_type_dependencies(ty: &StaticType, deps: &mut Vec<String>) {
        match ty {
            StaticType::Struct { name, .. } => deps.push(name.clone()),
            StaticType::Array { element, .. } | StaticType::Range { element } => {
                Self::collect_struct_type_dependencies(element, deps);
            }
            StaticType::Tuple(elements) => {
                for element in elements {
                    Self::collect_struct_type_dependencies(element, deps);
                }
            }
            StaticType::Dict { key, value } => {
                Self::collect_struct_type_dependencies(key, deps);
                Self::collect_struct_type_dependencies(value, deps);
            }
            StaticType::Function { params, ret } => {
                for param in params {
                    Self::collect_struct_type_dependencies(param, deps);
                }
                Self::collect_struct_type_dependencies(ret, deps);
            }
            StaticType::Union { variants } => {
                for variant in variants {
                    Self::collect_struct_type_dependencies(variant, deps);
                }
            }
            StaticType::DataType | StaticType::Any => {}
            _ => {}
        }
    }

    /// Emit an enum definition as i32 constants
    ///
    /// Julia enums (`@enum Color red green blue`) are backed by Int32.
    /// We emit them as Rust `const` values for zero-cost representation.
    pub(super) fn emit_enum(&mut self, e: &AotEnum) -> AotResult<()> {
        if self.config.emit_comments {
            self.write_line(&format!("// Julia @enum: {}", e.name));
        }

        // Type alias for the enum's backing type
        self.write_line(&format!("pub type {} = i32;", e.name));

        // Emit each member as a named constant, preserving the Julia binding
        // name (`red`, not `RED`) so references resolve. Non-upper-case const
        // names are allowed by the generated file's lint attributes (Issue #7050).
        for (member_name, value) in &e.members {
            self.write_line(&format!(
                "pub const {}: {} = {};",
                escape_rust_ident(member_name),
                e.name,
                value
            ));
        }

        Ok(())
    }

    /// Emit a global variable
    pub(super) fn emit_global(&mut self, global: &AotGlobal) -> AotResult<()> {
        let rust_ty = self.type_to_rust(&global.ty);

        if let Some(init) = &global.init {
            if !static_type_is_const_global_primitive(&global.ty) {
                return Err(AotError::UnsupportedInstruction(
                    UnsupportedInstructionDiagnostic::new(format!(
                        "global `{}` of type `{}` cannot be emitted as a const Rust static initializer",
                        global.name,
                        global.ty.julia_type_name()
                    ))
                    .with_workaround(
                        "wrap the binding in a local `let` block or use a scalar primitive global",
                    ),
                ));
            }
            let init_expr = self.emit_expr_to_string(init)?;
            // Prefix the static name so a function parameter of the same name
            // cannot shadow it (E0530; Issue #7242).
            self.write_line(&format!(
                "static {}: {} = {};",
                global_static_ident(&global.name),
                rust_ty,
                init_expr
            ));
        } else {
            return Err(AotError::UnsupportedInstruction(
                UnsupportedInstructionDiagnostic::new(format!(
                    "uninitialized global `{}` cannot be emitted as a Rust static",
                    global.name
                ))
                .with_workaround(
                    "initialize the global before AoT compilation or rewrite it as a local binding",
                ),
            ));
        }

        Ok(())
    }

    /// Find which variables (from a given set of parameter names) are reassigned in the body
    pub(super) fn find_reassigned_vars(
        &self,
        body: &[AotStmt],
        params: &[(String, StaticType)],
    ) -> HashSet<String> {
        let param_names: HashSet<_> = params.iter().map(|(name, _)| name.clone()).collect();
        let mut reassigned = HashSet::new();

        fn collect_from_stmts(
            stmts: &[AotStmt],
            param_names: &HashSet<String>,
            reassigned: &mut HashSet<String>,
        ) {
            for stmt in stmts {
                collect_from_stmt(stmt, param_names, reassigned);
            }
        }

        fn collect_from_stmt(
            stmt: &AotStmt,
            param_names: &HashSet<String>,
            reassigned: &mut HashSet<String>,
        ) {
            match stmt {
                // Check if target is a simple variable that matches a parameter
                AotStmt::Assign {
                    target: AotExpr::Var { name, .. },
                    ..
                } if param_names.contains(name) => {
                    reassigned.insert(name.clone());
                }
                AotStmt::CompoundAssign {
                    target: AotExpr::Var { name, .. },
                    ..
                } if param_names.contains(name) => {
                    reassigned.insert(name.clone());
                }
                AotStmt::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    collect_from_stmts(then_branch, param_names, reassigned);
                    if let Some(else_stmts) = else_branch {
                        collect_from_stmts(else_stmts, param_names, reassigned);
                    }
                }
                AotStmt::While { body, .. } => {
                    collect_from_stmts(body, param_names, reassigned);
                }
                AotStmt::ForRange { body, .. } => {
                    collect_from_stmts(body, param_names, reassigned);
                }
                AotStmt::ForEach { body, .. } => {
                    collect_from_stmts(body, param_names, reassigned);
                }
                _ => {}
            }
        }

        collect_from_stmts(body, &param_names, &mut reassigned);
        reassigned
    }

    /// Compute locals whose declaration must be hoisted to a deferred
    /// `let mut x: T;` at the top of the function.
    ///
    /// In Julia all locals share one function scope, but the IR converter emits
    /// a local's first assignment as an `AotStmt::Let` *at the point of that
    /// assignment*. When that point is inside an `if`/`while`/`for` block, the
    /// generated `let` is scoped to that Rust block, so any reference from
    /// another scope (a sibling branch, or after the block) fails to compile
    /// (`cannot find value`; Issue #8181). Such a local must instead be declared
    /// once at function scope and assigned in-block.
    ///
    /// A local is hoisted iff its first `Let` lives in a nested scope (depth ≥ 1)
    /// and it is referenced (read or written) from at least one other scope.
    /// Function parameters and loop-binding variables are never hoisted. The
    /// returned vector preserves first-declaration order for deterministic output.
    pub(super) fn compute_hoisted_locals(
        &self,
        body: &[AotStmt],
        params: &[(String, StaticType)],
    ) -> Vec<(String, StaticType)> {
        let param_names: HashSet<String> = params.iter().map(|(n, _)| n.clone()).collect();

        #[derive(Default)]
        struct Acc {
            next_scope: usize,
            let_scope: HashMap<String, usize>,
            let_ty: HashMap<String, StaticType>,
            ref_scopes: HashMap<String, HashSet<usize>>,
            order: Vec<String>,
            loop_vars: HashSet<String>,
        }

        fn read_expr(expr: &AotExpr, scope: usize, acc: &mut Acc) {
            let mut names = HashSet::new();
            collect_expr_reads(expr, &mut names);
            for name in names {
                acc.ref_scopes.entry(name).or_default().insert(scope);
            }
        }

        fn note_write(name: &str, scope: usize, acc: &mut Acc) {
            acc.ref_scopes
                .entry(name.to_string())
                .or_default()
                .insert(scope);
        }

        fn walk(stmts: &[AotStmt], scope: usize, acc: &mut Acc) {
            for stmt in stmts {
                match stmt {
                    AotStmt::Let {
                        name, ty, value, ..
                    } => {
                        read_expr(value, scope, acc);
                        note_write(name, scope, acc);
                        if !acc.let_scope.contains_key(name) {
                            acc.let_scope.insert(name.clone(), scope);
                            acc.let_ty.insert(name.clone(), ty.clone());
                            acc.order.push(name.clone());
                        }
                    }
                    AotStmt::Assign { target, value } => {
                        read_expr(value, scope, acc);
                        match target {
                            AotExpr::Var { name, .. } => note_write(name, scope, acc),
                            other => read_expr(other, scope, acc),
                        }
                    }
                    AotStmt::CompoundAssign { target, value, .. } => {
                        read_expr(value, scope, acc);
                        match target {
                            AotExpr::Var { name, .. } => note_write(name, scope, acc),
                            other => read_expr(other, scope, acc),
                        }
                    }
                    AotStmt::Expr(expr)
                    | AotStmt::ValueCarrier(expr)
                    | AotStmt::Return(Some(expr)) => read_expr(expr, scope, acc),
                    AotStmt::Return(None) | AotStmt::Break | AotStmt::Continue => {}
                    AotStmt::If {
                        condition,
                        then_branch,
                        else_branch,
                    } => {
                        read_expr(condition, scope, acc);
                        acc.next_scope += 1;
                        let then_scope = acc.next_scope;
                        walk(then_branch, then_scope, acc);
                        if let Some(else_branch) = else_branch {
                            acc.next_scope += 1;
                            let else_scope = acc.next_scope;
                            walk(else_branch, else_scope, acc);
                        }
                    }
                    AotStmt::While { condition, body } => {
                        read_expr(condition, scope, acc);
                        acc.next_scope += 1;
                        let body_scope = acc.next_scope;
                        walk(body, body_scope, acc);
                    }
                    AotStmt::ForRange {
                        var,
                        start,
                        stop,
                        step,
                        body,
                    } => {
                        read_expr(start, scope, acc);
                        read_expr(stop, scope, acc);
                        if let Some(step) = step {
                            read_expr(step, scope, acc);
                        }
                        acc.loop_vars.insert(var.clone());
                        acc.next_scope += 1;
                        let body_scope = acc.next_scope;
                        walk(body, body_scope, acc);
                    }
                    AotStmt::ForEach { var, iter, body } => {
                        read_expr(iter, scope, acc);
                        acc.loop_vars.insert(var.clone());
                        acc.next_scope += 1;
                        let body_scope = acc.next_scope;
                        walk(body, body_scope, acc);
                    }
                }
            }
        }

        let mut acc = Acc::default();
        walk(body, 0, &mut acc);

        acc.order
            .iter()
            .filter_map(|name| {
                if param_names.contains(name) || acc.loop_vars.contains(name) {
                    return None;
                }
                let let_scope = *acc.let_scope.get(name)?;
                if let_scope == 0 {
                    return None;
                }
                let ref_scopes = acc.ref_scopes.get(name)?;
                if ref_scopes.iter().any(|&s| s != let_scope) {
                    Some((name.clone(), acc.let_ty.get(name)?.clone()))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Emit a function definition
    pub(super) fn emit_function(
        &mut self,
        func: &AotFunction,
        direct_c_abi_export: bool,
    ) -> AotResult<()> {
        // Determine the function name to use
        // Use mangled name if this function has multiple dispatch methods
        let use_mangled = self.needs_dispatch(&func.name);
        let func_name = self.emitted_function_name(func);

        if self.config.emit_comments {
            if func.is_generic {
                self.write_line(&format!("// Generic function: {}", func.name));
            } else if use_mangled {
                self.write_line(&format!(
                    "// Function: {} (mangled: {})",
                    func.name, func_name
                ));
            } else {
                self.write_line(&format!("// Function: {}", func.name));
            }
        }

        // Find which parameters are reassigned in the function body
        let reassigned_params = self.find_reassigned_vars(&func.body, &func.params);

        // Function signature - add mut to reassigned parameters
        let params: Vec<_> = func
            .params
            .iter()
            .map(|(name, ty)| {
                let escaped = escape_rust_ident(name);
                if reassigned_params.contains(name) {
                    format!("mut {}: {}", escaped, self.type_to_rust(ty))
                } else {
                    format!("{}: {}", escaped, self.type_to_rust(ty))
                }
            })
            .collect();
        let return_ty = self.type_to_rust(&func.return_type);

        if direct_c_abi_export {
            self.write_line("#[no_mangle]");
        }
        self.write_line(&format!(
            "pub {}fn {}({}) -> {} {{",
            if direct_c_abi_export {
                "extern \"C\" "
            } else {
                ""
            },
            func_name,
            params.join(", "),
            return_ty
        ));
        self.indent();

        // Function body
        // The last statement may need special handling for implicit return
        let previous_return_type = self
            .current_function_return_type
            .replace(func.return_type.clone());
        // Track this function's parameter names so same-named global references
        // inside the body are NOT rewritten to the `__sjulia_global_<name>`
        // static (the parameter shadows the global; Issue #7242).
        let previous_param_names = std::mem::replace(
            &mut self.current_function_param_names,
            func.params.iter().map(|(name, _)| name.clone()).collect(),
        );
        // Hoist locals first-assigned inside a nested block but referenced from
        // another scope to a deferred `let mut x: T;` so their in-block `Let`s
        // (emitted as plain assignments) stay in scope everywhere (Issue #8181).
        let hoisted = self.compute_hoisted_locals(&func.body, &func.params);
        for (name, ty) in &hoisted {
            let rust_ty = self.type_to_rust(ty);
            self.write_line(&format!(
                "let mut {}: {};",
                escape_rust_ident(name),
                rust_ty
            ));
        }
        let previous_hoisted = std::mem::replace(
            &mut self.current_function_hoisted_locals,
            hoisted.into_iter().map(|(name, _)| name).collect(),
        );
        let body_result = (|| -> AotResult<()> {
            let body_len = func.body.len();
            for (i, stmt) in func.body.iter().enumerate() {
                let is_last = i == body_len - 1;
                if is_last {
                    self.emit_stmt_maybe_return(stmt, &func.return_type)?;
                } else {
                    self.emit_stmt(stmt)?;
                }
            }
            Ok(())
        })();
        self.current_function_return_type = previous_return_type;
        self.current_function_param_names = previous_param_names;
        self.current_function_hoisted_locals = previous_hoisted;
        body_result?;

        self.dedent();
        self.write_line("}");

        Ok(())
    }

    /// Emit main function
    pub(super) fn emit_main(&mut self, stmts: &[AotStmt]) -> AotResult<()> {
        self.write_line("pub fn main() {");
        self.indent();

        for stmt in stmts {
            self.emit_stmt(stmt)?;
        }

        self.dedent();
        self.write_line("}");

        Ok(())
    }
}

#[derive(Debug)]
struct DispatcherArm {
    specificity: usize,
    code: String,
}

#[derive(Debug, Clone)]
pub(super) struct ResolvedCAbiExport {
    pub(super) export_name: String,
    pub(super) rust_func_name: String,
    pub(super) func: AotFunction,
}
