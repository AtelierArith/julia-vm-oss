//! Statement compilation for CoreCompiler.
//!
//! This module contains statement-level compilation methods including
//! block, function body, and individual statement compilation.

use crate::ir::core::{BinaryOp, Block, Expr, Function, Literal, Stmt, UnaryOp};
use crate::types::JuliaType;

mod stmt_try_catch;
use crate::vm::value::is_array_wrapper_struct_name;
use crate::vm::{ArrayElementType, Instr, ValueType};

use super::types::{err, CResult, CompileError};
use super::{
    analyze_free_variables, collect_block_local_bindings, is_stdlib_module,
    static_assignment_types_compatible, CoreCompiler, LoopContext,
};
use std::collections::{HashMap, HashSet};

/// Evaluate a compile-time-constant integer step for a `for` range loop.
///
/// Returns `Some(k)` when the loop step is statically known to be the Int64 value
/// `k` (Issue #5166). The default (no explicit step) is `1`. Negative literals are
/// represented as a `UnaryOp::Neg` over a positive `Literal::Int` at this stage of
/// the pipeline (the lowering that turns `-1` into `NegInt` happens later, during
/// `compile_expr_as`), so they are matched directly here rather than via const_prop.
///
/// Returns `None` for any non-constant step (e.g. a variable or call), leaving the
/// caller to fall back to the dynamic per-iteration sign-check path.
fn const_int_step(step: &Option<Expr>) -> Option<i64> {
    match step {
        None => Some(1),
        Some(expr) => match expr {
            Expr::Literal(Literal::Int(k), _) => Some(*k),
            Expr::UnaryOp {
                op: crate::ir::core::UnaryOp::Neg,
                operand,
                ..
            } => match operand.as_ref() {
                Expr::Literal(Literal::Int(k), _) => k.checked_neg(),
                _ => None,
            },
            Expr::UnaryOp {
                op: crate::ir::core::UnaryOp::Pos,
                operand,
                ..
            } => match operand.as_ref() {
                Expr::Literal(Literal::Int(k), _) => Some(*k),
                _ => None,
            },
            _ => None,
        },
    }
}

fn literal_i64(expr: &Expr) -> Option<i64> {
    match expr {
        Expr::Literal(Literal::Int(v), _) => Some(*v),
        _ => None,
    }
}

fn is_unqualified_or_base_call(function: &str, name: &str) -> bool {
    function == name
        || function
            .strip_prefix("Base.")
            .is_some_and(|qualified| qualified == name)
}

fn is_dict_struct_name(name: &str) -> bool {
    // Split on `{` BEFORE stripping a module prefix: a parametric name like
    // `Dict{Symbolics.Num,Int64}` has a dot *inside* its type parameters, so
    // `rsplit('.')` on the whole string would wrongly yield `Num,Int64}`
    // (Issue #7173). Isolate the base (`Dict`) first, then drop any module
    // qualifier on it (`Base.Dict` -> `Dict`).
    let base = name.split('{').next().unwrap_or(name);
    let base = base.rsplit('.').next().unwrap_or(base);
    base == "Dict"
}

fn is_array_wrapper_compat_field(field: &str) -> bool {
    matches!(field, "_mem" | "_size")
}

fn eachindex_array_var(iterable: &Expr) -> Option<&str> {
    base_unary_call_array_var(iterable, &["eachindex"])
}

fn proven_inbounds_loop_array_var(iterable: &Expr) -> Option<&str> {
    eachindex_array_var(iterable)
        .or_else(|| axes_dim1_array_var(iterable))
        .or_else(|| one_to_length_array_var(iterable))
}

fn axes_dim1_array_var(iterable: &Expr) -> Option<&str> {
    match iterable {
        Expr::Call {
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            ..
        } if is_unqualified_or_base_call(function, "axes")
            && args.len() == 2
            && kwargs.is_empty()
            && splat_mask.iter().all(|s| !*s)
            && kwargs_splat_mask.iter().all(|s| !*s)
            && literal_i64(&args[1]) == Some(1) =>
        {
            match args.first()? {
                Expr::Var(name, _) => Some(name.as_str()),
                _ => None,
            }
        }
        Expr::ModuleCall {
            module,
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            ..
        } if module == "Base"
            && function == "axes"
            && args.len() == 2
            && kwargs.is_empty()
            && splat_mask.iter().all(|s| !*s)
            && kwargs_splat_mask.iter().all(|s| !*s)
            && literal_i64(&args[1]) == Some(1) =>
        {
            match args.first()? {
                Expr::Var(name, _) => Some(name.as_str()),
                _ => None,
            }
        }
        _ => None,
    }
}

fn one_to_length_array_var(iterable: &Expr) -> Option<&str> {
    match iterable {
        Expr::Call {
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            ..
        } if (is_unqualified_or_base_call(function, "OneTo")
            || is_unqualified_or_base_call(function, "oneto"))
            && args.len() == 1
            && kwargs.is_empty()
            && splat_mask.iter().all(|s| !*s)
            && kwargs_splat_mask.iter().all(|s| !*s) =>
        {
            base_unary_call_array_var(args.first()?, &["length", "lastindex"])
        }
        Expr::ModuleCall {
            module,
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            ..
        } if module == "Base"
            && (function == "OneTo" || function == "oneto")
            && args.len() == 1
            && kwargs.is_empty()
            && splat_mask.iter().all(|s| !*s)
            && kwargs_splat_mask.iter().all(|s| !*s) =>
        {
            base_unary_call_array_var(args.first()?, &["length", "lastindex"])
        }
        _ => None,
    }
}

fn base_unary_call_array_var<'a>(expr: &'a Expr, names: &[&str]) -> Option<&'a str> {
    match expr {
        Expr::Call {
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            ..
        } if names
            .iter()
            .any(|name| is_unqualified_or_base_call(function, name))
            && args.len() == 1
            && kwargs.is_empty()
            && splat_mask.iter().all(|s| !*s)
            && kwargs_splat_mask.iter().all(|s| !*s) =>
        {
            match args.first()? {
                Expr::Var(name, _) => Some(name.as_str()),
                _ => None,
            }
        }
        Expr::ModuleCall {
            module,
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            ..
        } if module == "Base"
            && names.contains(&function.as_str())
            && args.len() == 1
            && kwargs.is_empty()
            && splat_mask.iter().all(|s| !*s)
            && kwargs_splat_mask.iter().all(|s| !*s) =>
        {
            match args.first()? {
                Expr::Var(name, _) => Some(name.as_str()),
                _ => None,
            }
        }
        _ => None,
    }
}

fn positive_unit_length_loop_array_var<'a>(
    start: &Expr,
    end: &'a Expr,
    const_step: i64,
) -> Option<&'a str> {
    if const_step != 1 {
        return None;
    }

    let end_var = base_unary_call_array_var(end, &["length", "lastindex"])?;
    if literal_i64(start) == Some(1) {
        return Some(end_var);
    }

    let start_var = base_unary_call_array_var(start, &["firstindex"])?;
    (start_var == end_var).then_some(end_var)
}

/// Fold a pure, side-effect-free expression to a compile-time constant value.
///
/// Returns `Some(value)` only when the entire expression is built from constant
/// literals combined with pure arithmetic / comparison / boolean operators that
/// the const-evaluator (`compile::const_prop`) can evaluate. Any variable, call,
/// indexing, or unsupported operator yields `None` — folding is conservative so
/// it can never change observable behaviour.
///
/// Reuses the same `eval_const_binary` / `eval_const_unary` semantics that the
/// abstract interpreter relies on, so Julia-specific rules (truncated `%`, `÷`,
/// Int64 overflow checks, `/` producing Float64) stay in a single place.
#[cfg(test)]
#[allow(dead_code)]
fn fold_const_value(expr: &Expr) -> Option<crate::compile::lattice::types::ConstValue> {
    crate::compile::const_prop::fold_expr_const_value(expr, &|_| None)
}

/// Fold an `if`/ternary condition to a statically-known boolean when possible.
///
/// Powers dead-branch elimination (Issue #5182): conditions like `if 1 < 2`,
/// `if true && false`, or `if !false` collapse to a single branch at compile
/// time, removing the condition computation, the `JumpIfZero`, and the dead
/// branch's bytecode entirely. A bare `Expr::Literal(Literal::Bool(_))` is the
/// trivial case; this generalises it to any pure const-foldable expression that
/// evaluates to a `Bool`.
/// True only for conditions whose `Bool` value is determined WITHOUT any method
/// dispatch: literal `Bool`s and `&&`/`||`/`!` combinations of such.
///
/// Comparison/equality operators (`==`, `!=`, `<`, `<=`, `>`, `>=`, `<:`) and
/// `arithmetic` all dispatch to methods that user code may override
/// (Issue #4298 — e.g. a user `==(::String, ::String)`), so a condition that
/// contains one is NOT safe to const-fold for dead-branch elimination: the
/// runtime value can differ from the literal fold. `&&`/`||` (`BinaryOp::And`/
/// `Or`) are short-circuit control flow, not method calls, so they are safe.
#[cfg(test)]
#[allow(dead_code)]
fn is_dispatch_free_bool_condition(expr: &Expr) -> bool {
    is_dispatch_free_bool_condition_with_lookup(expr, &|_| None)
}

fn is_dispatch_free_bool_condition_with_lookup<F>(expr: &Expr, lookup_const: &F) -> bool
where
    F: Fn(&str) -> Option<crate::compile::lattice::types::ConstValue>,
{
    use crate::ir::core::{BinaryOp, UnaryOp};
    match expr {
        Expr::Literal(Literal::Bool(_), _) => true,
        Expr::Var(name, _) => matches!(
            lookup_const(name),
            Some(crate::compile::lattice::types::ConstValue::Bool(_))
        ),
        Expr::UnaryOp {
            op: UnaryOp::Not,
            operand,
            ..
        } => is_dispatch_free_bool_condition_with_lookup(operand, lookup_const),
        Expr::BinaryOp {
            op: BinaryOp::And | BinaryOp::Or,
            left,
            right,
            ..
        } => {
            is_dispatch_free_bool_condition_with_lookup(left, lookup_const)
                && is_dispatch_free_bool_condition_with_lookup(right, lookup_const)
        }
        _ => false,
    }
}

#[cfg(test)]
#[allow(dead_code)]
pub(super) fn const_bool_condition(condition: &Expr) -> Option<bool> {
    const_bool_condition_with_lookup(condition, &|_| None)
}

pub(super) fn const_bool_condition_with_lookup<F>(
    condition: &Expr,
    lookup_const: &F,
) -> Option<bool>
where
    F: Fn(&str) -> Option<crate::compile::lattice::types::ConstValue>,
{
    // Dead-branch elimination (Issue #5182) must only fire when the condition's
    // `Bool` value is independent of method dispatch. Folding comparison/equality
    // operators here discards user-overridden methods: `if "a" == "a"` with a
    // user `==(::String, ::String) = false` was being eliminated to the wrong
    // (then) branch, regressing Issue #4298. Restrict to dispatch-free conditions.
    if !is_dispatch_free_bool_condition_with_lookup(condition, lookup_const) {
        return None;
    }
    match crate::compile::const_prop::fold_expr_const_value(condition, lookup_const)? {
        crate::compile::lattice::types::ConstValue::Bool(b) => Some(b),
        _ => None,
    }
}

/// Check if a direct type conversion is possible between two value types.
///
/// Only I64↔F64 conversions are supported by dedicated VM instructions
/// (ToF64 and ToI64). All other type coercions go through Pure Julia `convert()`.
pub(super) fn can_convert_type(from: ValueType, to: ValueType) -> bool {
    matches!(
        (from, to),
        (ValueType::I64, ValueType::F64) | (ValueType::F64, ValueType::I64)
    )
}

fn target_preserves_boxed_numeric_values(target_ty: Option<&ValueType>) -> bool {
    matches!(
        target_ty,
        Some(
            ValueType::MemoryOf(
                ArrayElementType::Any
                    | ArrayElementType::UnionOf(_)
                    | ArrayElementType::Abstract(_)
            ) | ValueType::ArrayOf(
                ArrayElementType::Any
                    | ArrayElementType::UnionOf(_)
                    | ArrayElementType::Abstract(_),
                _,
            ) | ValueType::Struct(_)
                | ValueType::Any
        )
    )
}

fn should_return_as_expected_type(actual_ty: &ValueType, expected_ty: &ValueType) -> bool {
    actual_ty == expected_ty
        || matches!(expected_ty, ValueType::Any)
        || (matches!(actual_ty, ValueType::Any)
            && matches!(
                expected_ty,
                ValueType::I64 | ValueType::F64 | ValueType::F32 | ValueType::F16
            ))
}

fn const_declaration_marker(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Call { function, args, .. }
            if function == "#__sjulia_declare_const__" && args.len() == 1 =>
        {
            match &args[0] {
                Expr::Literal(Literal::Str(name), _) => Some(name.as_str()),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Determine the iteration strategy for a type known at compile time.
///
/// Returns:
/// - `Some(true)`  — call Pure Julia `iterate()` (custom struct iterators, `Any` dispatch)
/// - `Some(false)` — emit a VM builtin instruction (faster path for known collections)
/// - `None`        — type is unknown; requires a runtime method-table lookup
///
/// The `None` case is handled by `should_use_pure_julia_iterate`, which falls back to
/// checking `self.method_tables` at compile time.
pub(super) fn static_iterate_strategy(ty: &JuliaType) -> Option<bool> {
    match ty {
        // CartesianIndices uses VM builtin iterate for better performance
        JuliaType::Struct(name) if name == "CartesianIndices" => Some(false),
        // All other struct types use Pure Julia iterate (custom iterators)
        JuliaType::Struct(_) => Some(true),
        // Any type: use Pure Julia dispatch (handles unknown runtime structs)
        JuliaType::Any => Some(true),
        // Builtin collection types: faster VM instructions
        JuliaType::Array | JuliaType::VectorOf(_) | JuliaType::MatrixOf(_) => Some(false),
        JuliaType::Tuple | JuliaType::TupleOf(_) => Some(false),
        JuliaType::String => Some(false),
        JuliaType::Int64 => Some(false), // Range-like types
        // Unknown type; let the caller perform a dynamic method-table lookup
        _ => None,
    }
}

impl CoreCompiler<'_> {
    pub(super) fn compile_block(&mut self, block: &Block) -> CResult<()> {
        for stmt in &block.stmts {
            self.compile_stmt(stmt)?;
        }
        Ok(())
    }

    fn compile_condition_value(&mut self, condition: &Expr) -> CResult<ValueType> {
        match condition {
            Expr::LetBlock { bindings, body, .. } if bindings.is_empty() => {
                self.compile_block_as_condition_value(body)
            }
            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => self.compile_ternary_as_condition_value(condition, then_expr, else_expr),
            _ => self.compile_expr(condition),
        }
    }

    fn compile_block_as_condition_value(&mut self, block: &Block) -> CResult<ValueType> {
        let stmts = &block.stmts;
        if stmts.is_empty() {
            self.emit(Instr::PushNothing);
            return Ok(ValueType::Nothing);
        }

        for stmt in stmts.iter().take(stmts.len() - 1) {
            self.compile_stmt(stmt)?;
        }

        match &stmts[stmts.len() - 1] {
            Stmt::Expr { expr, .. } => self.compile_condition_value(expr),
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => self.compile_if_as_condition_value(condition, then_branch, else_branch.as_ref()),
            Stmt::Block(block) => self.compile_block_as_condition_value(block),
            stmt => {
                self.compile_stmt(stmt)?;
                self.emit(Instr::PushNothing);
                Ok(ValueType::Nothing)
            }
        }
    }

    fn compile_ternary_as_condition_value(
        &mut self,
        condition: &Expr,
        then_expr: &Expr,
        else_expr: &Expr,
    ) -> CResult<ValueType> {
        let false_jumps = self.compile_condition_false_jumps(condition)?;
        let then_ty = self.compile_condition_value(then_expr)?;
        let jump_end = self.here();
        self.emit(Instr::Jump(usize::MAX));

        let else_start = self.here();
        for patch_pos in false_jumps {
            self.patch_jump(patch_pos, else_start);
        }

        let else_ty = self.compile_condition_value(else_expr)?;
        let end = self.here();
        self.patch_jump(jump_end, end);

        if then_ty == else_ty {
            Ok(then_ty)
        } else {
            Ok(ValueType::Any)
        }
    }

    fn compile_if_as_condition_value(
        &mut self,
        condition: &Expr,
        then_branch: &Block,
        else_branch: Option<&Block>,
    ) -> CResult<ValueType> {
        let false_jumps = self.compile_condition_false_jumps(condition)?;
        let then_ty = self.compile_block_as_condition_value(then_branch)?;
        let jump_end = self.here();
        self.emit(Instr::Jump(usize::MAX));

        let else_start = self.here();
        for patch_pos in false_jumps {
            self.patch_jump(patch_pos, else_start);
        }

        let else_ty = if let Some(else_branch) = else_branch {
            self.compile_block_as_condition_value(else_branch)?
        } else {
            self.emit(Instr::PushNothing);
            ValueType::Nothing
        };

        let end = self.here();
        self.patch_jump(jump_end, end);

        if then_ty == else_ty {
            Ok(then_ty)
        } else {
            Ok(ValueType::Any)
        }
    }

    /// Compile a condition in branch context, returning jumps to patch to the
    /// false target. The generated code falls through when the condition is
    /// true and leaves no Bool value on the stack.
    ///
    /// This keeps `if`/`while` conditions from materializing `&&` / `||` as
    /// stack Bool values. For `a && b`, false exits are emitted directly after
    /// each operand; for `a || b`, a true left operand skips the right operand.
    /// Leaf conditions still use `JumpIfZero`, preserving the VM's Bool-only
    /// control-flow check instead of treating numbers as truthy (Issue #6162).
    pub(in crate::compile) fn compile_condition_false_jumps(
        &mut self,
        condition: &Expr,
    ) -> CResult<Vec<usize>> {
        match condition {
            Expr::Literal(Literal::Bool(true), _) => Ok(Vec::new()),
            Expr::Literal(Literal::Bool(false), _) => {
                let j_false = self.here();
                self.emit(Instr::Jump(usize::MAX));
                Ok(vec![j_false])
            }
            Expr::UnaryOp {
                op: UnaryOp::Not,
                operand,
                ..
            } => self.compile_condition_true_jumps(operand),
            Expr::BinaryOp {
                op: BinaryOp::And,
                left,
                right,
                ..
            } => {
                let mut false_jumps = self.compile_condition_false_jumps(left)?;
                false_jumps.extend(self.compile_condition_false_jumps(right)?);
                Ok(false_jumps)
            }
            Expr::BinaryOp {
                op: BinaryOp::Or,
                left,
                right,
                ..
            } => {
                let true_jumps = self.compile_condition_true_jumps(left)?;
                let false_jumps = self.compile_condition_false_jumps(right)?;
                let true_start = self.here();
                for patch_pos in true_jumps {
                    self.patch_jump(patch_pos, true_start);
                }
                Ok(false_jumps)
            }
            _ => {
                self.compile_condition_value(condition)?;
                let j_false = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX));
                Ok(vec![j_false])
            }
        }
    }

    fn compile_condition_true_jumps(&mut self, condition: &Expr) -> CResult<Vec<usize>> {
        match condition {
            Expr::Literal(Literal::Bool(true), _) => {
                let j_true = self.here();
                self.emit(Instr::Jump(usize::MAX));
                Ok(vec![j_true])
            }
            Expr::Literal(Literal::Bool(false), _) => Ok(Vec::new()),
            Expr::UnaryOp {
                op: UnaryOp::Not,
                operand,
                ..
            } => self.compile_condition_false_jumps(operand),
            Expr::BinaryOp {
                op: BinaryOp::And,
                left,
                right,
                ..
            } => {
                let false_jumps = self.compile_condition_false_jumps(left)?;
                let true_jumps = self.compile_condition_true_jumps(right)?;
                let false_start = self.here();
                for patch_pos in false_jumps {
                    self.patch_jump(patch_pos, false_start);
                }
                Ok(true_jumps)
            }
            Expr::BinaryOp {
                op: BinaryOp::Or,
                left,
                right,
                ..
            } => {
                let mut true_jumps = self.compile_condition_true_jumps(left)?;
                true_jumps.extend(self.compile_condition_true_jumps(right)?);
                Ok(true_jumps)
            }
            _ => {
                self.compile_condition_value(condition)?;
                let j_false = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX));
                let j_true = self.here();
                self.emit(Instr::Jump(usize::MAX));
                let false_start = self.here();
                self.patch_jump(j_false, false_start);
                Ok(vec![j_true])
            }
        }
    }

    /// Refine `self.locals` for the duration of a guarded `then` branch
    /// (Issue #5181). Recognizes `x isa T` / `c1 && c2` guards and overlays a
    /// concrete [`ValueType`] for each narrowed variable so that loads and
    /// arithmetic inside the branch specialize.
    ///
    /// Returns a restore snapshot: for every refined variable, the
    /// `(name, narrowed_type, previous_binding)` triple. Pass it to
    /// [`Self::restore_then_narrowings`] right after the branch is compiled so
    /// the refinement never leaks past the branch.
    ///
    /// Variables that are abstract-numeric params or captured closure vars are
    /// skipped: those are always loaded via `LoadAny`/`LoadCaptured` regardless
    /// of `self.locals`, so refining them would be inert at best and risk
    /// confusing downstream return-type handling.
    pub(super) fn apply_then_narrowings(
        &mut self,
        condition: &Expr,
    ) -> Vec<(String, ValueType, Option<ValueType>)> {
        let struct_id_for = |name: &str| self.shared_ctx.get_struct_type_id(name);
        let current_type_for = |name: &str| self.locals.get(name).cloned();
        let facts = super::narrowing::then_branch_narrowings_with_current(
            condition,
            &current_type_for,
            &struct_id_for,
        );
        self.apply_branch_narrowing_facts(facts)
    }

    /// Refine `self.locals` for the duration of a guarded `else` branch when
    /// union splitting proves the negated guard has a single concrete type
    /// (Issue #5077).
    pub(super) fn apply_else_narrowings(
        &mut self,
        condition: &Expr,
    ) -> Vec<(String, ValueType, Option<ValueType>)> {
        let struct_id_for = |name: &str| self.shared_ctx.get_struct_type_id(name);
        let current_type_for = |name: &str| self.locals.get(name).cloned();
        let facts =
            super::narrowing::else_branch_narrowings(condition, &current_type_for, &struct_id_for);
        self.apply_branch_narrowing_facts(facts)
    }

    fn apply_branch_narrowing_facts(
        &mut self,
        facts: Vec<(String, ValueType)>,
    ) -> Vec<(String, ValueType, Option<ValueType>)> {
        let mut restore = Vec::new();
        for (name, narrowed) in facts {
            if self.abstract_numeric_params.contains(&name) || self.captured_vars.contains(&name) {
                continue;
            }
            // Only refine when the variable is an actual local whose current
            // static type is strictly less precise than the narrowed type.
            // Refining an already-concrete or unrelated typed slot could only
            // mistype it, so we leave those alone.
            match self.locals.get(&name) {
                Some(ValueType::Any) | Some(ValueType::Union(_)) => {}
                _ => continue,
            }
            let prev = self.locals.insert(name.clone(), narrowed.clone());
            restore.push((name, narrowed, prev));
        }
        restore
    }

    /// Undo the refinements applied by branch narrowing.
    ///
    /// If the branch reassigned a narrowed variable, `self.locals` no longer
    /// holds the narrowed type we inserted — Julia variables are function-scoped
    /// so that assignment must persist past the branch. We therefore only roll a
    /// variable back when its current binding is still exactly the narrowed type
    /// we installed (i.e. the branch only *read* it).
    pub(super) fn restore_then_narrowings(
        &mut self,
        restore: Vec<(String, ValueType, Option<ValueType>)>,
    ) {
        for (name, narrowed, prev) in restore {
            if self.locals.get(&name) != Some(&narrowed) {
                // The branch rebound the variable; keep its post-branch type.
                continue;
            }
            match prev {
                Some(ty) => {
                    self.locals.insert(name, ty);
                }
                None => {
                    self.locals.remove(&name);
                }
            }
        }
    }

    /// Undo branch refinements even when the guarded expression assigned to the
    /// narrowed variable. This is used for short-circuit value expressions:
    /// `cond && (x = ...)` only executes the assignment on one path, so keeping
    /// the RHS-only slot type after the expression is unsound (Issue #7546).
    pub(super) fn restore_short_circuit_narrowings(
        &mut self,
        restore: Vec<(String, ValueType, Option<ValueType>)>,
    ) {
        for (name, _narrowed, prev) in restore {
            match prev {
                Some(ty) => {
                    self.locals.insert(name, ty);
                }
                None => {
                    self.locals.remove(&name);
                }
            }
        }
    }

    /// Compile an integer range `for` loop whose step is a compile-time constant
    /// (Issue #5166).
    ///
    /// Because the step sign is statically known, the per-iteration sign check is
    /// hoisted out entirely: a positive step emits a single `JumpIfGtI64` exit test
    /// (exit when `var > stop`) and a negative step emits `JumpIfLtI64` (exit when
    /// `var < stop`). The increment is specialized to `IncVarI64` / `DecVarI64`
    /// (with a `PushI64` of the magnitude for non-unit steps).
    ///
    /// The user-provided `stop` is stored verbatim (no `last` precompute), so the
    /// number of iterations and overflow/wrapping behavior match the dynamic path.
    /// `const_step` must be non-zero (callers route zero steps to the dynamic path).
    fn compile_const_step_for(
        &mut self,
        var: &str,
        start: &Expr,
        end: &Expr,
        const_step: i64,
        body: &Block,
    ) -> CResult<()> {
        debug_assert!(const_step != 0, "zero step must use the dynamic path");

        let stop_var = self.new_temp("stop");

        // Compile and store the (user-provided) stop value.
        self.compile_expr_as(end, ValueType::I64)?;
        self.emit(Instr::StoreI64(stop_var.clone()));

        // Initialize loop variable to start.
        self.compile_expr_as(start, ValueType::I64)?;
        self.emit(Instr::StoreI64(var.to_string()));

        let loop_start = self.here();

        let mut loop_ctx = LoopContext {
            exit_patches: Vec::new(),
            continue_patches: Vec::new(),
        };

        // Single-direction exit test. The step sign is known at compile time:
        //   step > 0: exit when var > stop  (JumpIfGtI64)
        //   step < 0: exit when var < stop  (JumpIfLtI64)
        self.emit(Instr::LoadI64(var.to_string()));
        self.emit(Instr::LoadI64(stop_var));
        let j_exit = self.here();
        if const_step > 0 {
            self.emit(Instr::JumpIfGtI64(usize::MAX));
        } else {
            self.emit(Instr::JumpIfLtI64(usize::MAX));
        }
        loop_ctx.exit_patches.push(j_exit);

        // Compile body with loop context.
        let inbounds_array_var = positive_unit_length_loop_array_var(start, end, const_step);
        if let Some(array_var) = inbounds_array_var {
            self.push_proven_inbounds_index(array_var, var);
        }
        self.loop_stack.push(loop_ctx);
        let body_result = self.compile_block(body);
        let loop_ctx = self.loop_stack.pop().unwrap();
        if inbounds_array_var.is_some() {
            self.pop_proven_inbounds_index();
        }
        body_result?;

        let continue_target = self.here();

        // Constant increment. `IncVarI64`/`DecVarI64` pop the (de/in)crement from the
        // stack and wrapping-add/sub it into the slot, matching the AddI64 wrapping
        // semantics of the dynamic path. We push the magnitude (always >= 1) and use
        // `IncVarI64` for positive steps and `DecVarI64` for negative steps so the
        // single-direction loop never needs the step's sign at runtime.
        if const_step > 0 {
            self.emit(Instr::PushI64(const_step));
            self.emit(Instr::IncVarI64(var.to_string()));
        } else {
            // step < 0: decrement by |step|. `const_step` is negative and non-zero;
            // negate it to obtain a positive magnitude. `i64::MIN` cannot reach here:
            // it has no positive literal counterpart, so `const_int_step` returns
            // `None` (via `checked_neg`) for that pathological case and the loop falls
            // back to the dynamic path. Hence the negation below cannot overflow.
            let magnitude = const_step
                .checked_neg()
                .expect("non-zero constant step magnitude must be representable");
            self.emit(Instr::PushI64(magnitude));
            self.emit(Instr::DecVarI64(var.to_string()));
        }

        self.emit(Instr::Jump(loop_start));

        let exit = self.here();
        for patch_pos in loop_ctx.exit_patches {
            self.patch_jump(patch_pos, exit);
        }
        for patch_pos in loop_ctx.continue_patches {
            self.patch_jump(patch_pos, continue_target);
        }

        Ok(())
    }

    /// Compile a function body with implicit return handling.
    /// In Julia, the last expression in a function is its return value.
    /// Issue #8118: pre-scan a function body's directly-nested function
    /// definitions and transitively propagate the captured locals of sibling
    /// closures into the capture set of every nested function that
    /// (transitively) calls them.
    ///
    /// A nested function `b` that captures an enclosing local `s` becomes a
    /// closure invoked through its captured environment. A sibling `a` that
    /// calls `b` must be able to reconstruct `b`'s environment at the call site
    /// (see `compile_self_or_sibling_closure_call`), which requires `a` to hold
    /// every local `b` captured — even when `a` does not reference those locals
    /// directly. Without this, mutually-recursive closures that capture an
    /// enclosing local fail at runtime with `Unknown function: <sibling>`
    /// (PR #8142 fixed the self-recursive and capture-free mutual cases; this
    /// covers the remaining capture-an-enclosing-local mutual case).
    ///
    /// We compute each nested function's base free variables against the *full*
    /// enclosing local scope (the body's statements have not been compiled yet,
    /// so `self.locals` lacks them), then fixpoint-union the captures of every
    /// called sibling, and merge the expanded sets into
    /// `shared_ctx.closure_captures` so both the `CreateClosure` emission below
    /// and the per-function `captured_vars` setup observe them.
    fn prescan_mutual_closure_captures(&mut self, block: &Block) {
        // Closures only capture *enclosing locals*, which exist inside a
        // function body (strict scope), not at module top level.
        if !self.strict_undefined_check {
            return;
        }
        let Some(parent) = self.current_function_name.clone() else {
            return;
        };

        // Directly-nested function definitions in this block.
        let nested: Vec<&Function> = block
            .stmts
            .iter()
            .filter_map(|stmt| match stmt {
                Stmt::FunctionDef { func, .. } => Some(func.as_ref()),
                _ => None,
            })
            .collect();
        // Sibling mutual recursion needs at least two nested functions; a single
        // self-recursive closure is already handled by reconstruction.
        if nested.len() < 2 {
            return;
        }

        // Full enclosing local scope: params + already-captured names + every
        // name bound anywhere in the parent body (regardless of source order).
        let mut outer_scope_vars: HashSet<String> = self.locals.keys().cloned().collect();
        outer_scope_vars.extend(self.captured_vars.iter().cloned());
        outer_scope_vars.extend(collect_block_local_bindings(block));

        let nested_names: HashSet<String> = nested.iter().map(|f| f.name.clone()).collect();

        // Base captures (enclosing-scope DATA variables only) + called-sibling
        // references for each nested function. Sibling function names are
        // EXCLUDED from captures: a sibling is resolved at the call site, either
        // by name (plain nested function) or by reconstructing its closure from
        // the captures both siblings now share — never data-captured. Capturing
        // a sibling's name would make a mutually-recursive group uncapturable
        // (each name's value is another not-yet-built closure).
        let mut caps: HashMap<String, HashSet<String>> = HashMap::new();
        let mut called: HashMap<String, HashSet<String>> = HashMap::new();
        for f in &nested {
            let base: HashSet<String> = analyze_free_variables(f, &outer_scope_vars)
                .into_iter()
                .filter(|name| !nested_names.contains(name))
                .collect();
            let refs: HashSet<String> =
                crate::compile::ipo::call_graph::extract_called_functions(&f.body)
                    .into_iter()
                    .filter(|name| nested_names.contains(name) && name != &f.name)
                    .collect();
            caps.insert(f.name.clone(), base);
            called.insert(f.name.clone(), refs);
        }

        // Fixpoint: a function that calls a sibling closure must capture
        // everything that sibling captures. Iterate until no set grows.
        loop {
            let mut changed = false;
            let names: Vec<String> = caps.keys().cloned().collect();
            for name in &names {
                let siblings = called[name].clone();
                let mut additions: HashSet<String> = HashSet::new();
                for sib in &siblings {
                    if let Some(sib_caps) = caps.get(sib) {
                        for c in sib_caps {
                            if !caps[name].contains(c) {
                                additions.insert(c.clone());
                            }
                        }
                    }
                }
                if !additions.is_empty() {
                    if let Some(set) = caps.get_mut(name) {
                        set.extend(additions);
                    }
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        // Record the expanded capture sets as authoritative for this body's
        // FunctionDef compilation. Only non-empty sets matter: an empty set is a
        // plain nested function (or a capture-free mutual group like the cases
        // PR #8142 already handles) and is left to the existing free-variable
        // path, keeping this change scoped to the capture-an-enclosing-local
        // mutual-recursion case.
        for f in &nested {
            let Some(expanded) = caps.get(&f.name) else {
                continue;
            };
            if expanded.is_empty() {
                continue;
            }
            let qualified = format!("{}#{}", parent, f.name);
            self.mutual_closure_captures
                .insert(qualified, expanded.clone());
        }
    }

    pub(super) fn compile_function_body(
        &mut self,
        block: &Block,
        return_type: ValueType,
    ) -> CResult<()> {
        // Pre-scan the body for `global x` declarations so that reads and writes
        // of those names route to the module-level frame for the whole scope,
        // matching upstream Julia (Issues #5548, #5549). A `global` declaration
        // applies to the entire local scope regardless of its position, so this
        // must happen before any statement is compiled. This only matters inside
        // a function: at module scope the binding is *already* global, so a
        // `global x` there is a no-op and routing it through `declared_globals`
        // would needlessly widen the variable's type to `Any`.
        if self.strict_undefined_check {
            collect_declared_globals(block, &mut self.declared_globals);
        }

        // Issue #8118: propagate sibling closures' captures so mutually-recursive
        // nested closures that capture an enclosing local can reconstruct each
        // other at their call sites. Must run before any FunctionDef statement is
        // compiled (it emits CreateClosure from the capture set).
        self.prescan_mutual_closure_captures(block);

        let stmts = &block.stmts;

        if stmts.is_empty() {
            // Empty function - return default value
            self.emit_default_return(return_type);
            return Ok(());
        }

        // Compile all statements except the last one normally
        for stmt in &stmts[..stmts.len() - 1] {
            self.compile_stmt(stmt)?;
        }

        // Handle the last statement specially
        let last_stmt = &stmts[stmts.len() - 1];
        match last_stmt {
            Stmt::Return {
                value: Some(expr), ..
            } => {
                // Explicit return with value - compile and return it
                let ty = self.compile_expr(expr)?;
                if should_return_as_expected_type(&ty, &return_type) {
                    self.emit_return_for_type(return_type);
                } else {
                    self.emit_return_for_type(ty);
                }
            }
            Stmt::Return { value: None, .. } => {
                // Explicit return without value
                self.emit(Instr::ReturnNothing);
            }
            Stmt::Expr { expr, .. } => {
                // Implicit return - the last expression is the return value
                let actual_ty = self.compile_expr(expr)?;
                // Try to convert to the declared return type if needed
                if actual_ty != return_type
                    && can_convert_type(actual_ty.clone(), return_type.clone())
                {
                    self.emit_type_conversion(actual_ty, return_type.clone());
                    self.emit_return_for_type(return_type);
                } else if should_return_as_expected_type(&actual_ty, &return_type) {
                    self.emit_return_for_type(return_type);
                } else {
                    // Use the actual type when conversion isn't possible
                    // This handles DataType returns and other non-convertible types
                    self.emit_return_for_type(actual_ty);
                }
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                // If statement as last statement in function - handle implicit return
                // Each branch should return its last expression's value
                self.compile_if_with_implicit_return(
                    condition,
                    then_branch,
                    else_branch.as_ref(),
                    return_type,
                )?;
            }
            Stmt::Try { .. } => {
                // A `try/catch[/else/finally]` in tail position is an expression
                // whose value is the last expression of whichever branch ran, not
                // the type's default value (Issue #6223).
                self.compile_try_with_implicit_return(last_stmt, return_type)?;
            }
            Stmt::Block(block) => {
                self.compile_block_with_implicit_return(block, return_type)?;
            }
            Stmt::FunctionDef { func, .. } => {
                self.compile_stmt(last_stmt)?;
                self.emit(Instr::LoadAny(func.name.clone()));
                self.emit(Instr::ReturnAny);
            }
            Stmt::EvalFunctionDef { .. } => {
                self.compile_stmt(last_stmt)?;
                self.emit_default_return(return_type);
            }
            _ => {
                // Other statements (while, for, etc.) - compile normally and add default return
                self.compile_stmt(last_stmt)?;
                self.emit_default_return(return_type);
            }
        }

        Ok(())
    }

    fn emit_default_return(&mut self, return_type: ValueType) {
        match return_type {
            ValueType::I64 => {
                self.emit(Instr::PushI64(0));
                self.emit(Instr::ReturnI64);
            }
            ValueType::F64 => {
                self.emit(Instr::PushF64(0.0));
                self.emit(Instr::ReturnF64);
            }
            ValueType::Struct(_type_id) => {
                // For struct return types without explicit return, return Nothing
                self.emit(Instr::ReturnNothing);
            }
            _ => {
                self.emit(Instr::ReturnNothing);
            }
        }
    }

    pub(super) fn emit_return_for_type(&mut self, ty: ValueType) {
        match ty {
            ValueType::I64 => self.emit(Instr::ReturnI64),
            ValueType::F64 => self.emit(Instr::ReturnF64),
            ValueType::Array | ValueType::ArrayOf(_, _) => self.emit(Instr::ReturnArray),
            ValueType::Str => self.emit(Instr::ReturnAny), // String uses dynamic return
            // Nothing type: use ReturnAny to consume the Nothing value pushed by compile_expr.
            // ReturnNothing does NOT pop the stack, so using it here would leave an orphaned
            // Nothing on the stack, corrupting nested call chains (Issue #2072).
            ValueType::Nothing => self.emit(Instr::ReturnAny),
            ValueType::Missing => self.emit(Instr::ReturnAny),
            ValueType::Struct(_) | ValueType::ComplexF32 | ValueType::ComplexF64 => {
                self.emit(Instr::ReturnStruct)
            }
            ValueType::Rng => self.emit(Instr::ReturnRng),
            ValueType::Range => self.emit(Instr::ReturnRange),
            ValueType::Tuple => self.emit(Instr::ReturnTuple),
            ValueType::NamedTuple => self.emit(Instr::ReturnNamedTuple),
            ValueType::Dict | ValueType::Set => self.emit(Instr::ReturnDict),
            ValueType::Generator => self.emit(Instr::ReturnAny),
            ValueType::Char => self.emit(Instr::ReturnAny),
            ValueType::Any => self.emit(Instr::ReturnAny),
            ValueType::DataType => self.emit(Instr::ReturnAny),
            ValueType::Module => self.emit(Instr::ReturnAny),
            ValueType::BigInt => self.emit(Instr::ReturnAny),
            ValueType::BigFloat => self.emit(Instr::ReturnAny),
            ValueType::IO => self.emit(Instr::ReturnAny),
            ValueType::Function => self.emit(Instr::ReturnAny),
            // Narrow integer types: ReturnI64 handler already preserves the original Value type
            // (I8/I16/I32/I128/U8–U128/Bool) via `preserved_val`, so using ReturnI64 is safe
            // and informs the AoT compiler that the return type is integer-family. (Issue #3255)
            ValueType::I8
            | ValueType::I16
            | ValueType::I32
            | ValueType::I128
            | ValueType::U8
            | ValueType::U16
            | ValueType::U32
            | ValueType::U64
            | ValueType::U128
            | ValueType::Bool => self.emit(Instr::ReturnI64),
            ValueType::F32 => self.emit(Instr::ReturnF32),
            ValueType::F16 => self.emit(Instr::ReturnF16),
            // Macro system types
            ValueType::Symbol
            | ValueType::Expr
            | ValueType::QuoteNode
            | ValueType::LineNumberNode
            | ValueType::GlobalRef => self.emit(Instr::ReturnAny),
            // Pairs type (for kwargs...)
            ValueType::Pairs => self.emit(Instr::ReturnAny),
            // Regex types
            ValueType::Regex | ValueType::RegexMatch => self.emit(Instr::ReturnAny),
            // Enum type
            ValueType::Enum => self.emit(Instr::ReturnAny),
            // Union type
            ValueType::Union(_) => self.emit(Instr::ReturnAny),
            // Memory type
            ValueType::Memory | ValueType::MemoryOf(_) => self.emit(Instr::ReturnAny),
        }
    }

    /// Emit type conversion instructions from actual to target type.
    /// Note: Complex conversions are handled via Pure Julia convert() functions.
    fn emit_type_conversion(&mut self, from: ValueType, to: ValueType) {
        match (from, to) {
            (ValueType::I64, ValueType::F64) => self.emit(Instr::ToF64),
            (ValueType::F64, ValueType::I64) => self.emit(Instr::ToI64),
            // Other conversions are not needed or not possible
            _ => {}
        }
    }

    /// Compile an if statement as the last statement in a function with implicit return.
    /// Each branch returns its last expression's value instead of falling through.
    fn compile_if_with_implicit_return(
        &mut self,
        condition: &Expr,
        then_branch: &Block,
        else_branch: Option<&Block>,
        return_type: ValueType,
    ) -> CResult<()> {
        // Dead code elimination: skip provably dead branches.
        // Fires on a bare Bool literal (Issue #3364) and, via the const-bool
        // folder, on any pure const-foldable condition such as `if 1 < 2` or
        // `if true && false` (Issue #5182).
        if let Some(b) = const_bool_condition_with_lookup(condition, &|name| {
            self.const_values.get(name).cloned()
        }) {
            if b {
                // Condition is always true: only compile then-branch
                self.compile_block_with_implicit_return(then_branch, return_type)?;
            } else if let Some(else_block) = else_branch {
                // Condition is always false: only compile else-branch
                self.compile_block_with_implicit_return(else_block, return_type)?;
            } else {
                // Condition is always false, no else: return default
                self.emit_default_return(return_type);
            }
            return Ok(());
        }

        let condition_false_jumps = self.compile_condition_false_jumps(condition)?;

        // Compile then-branch with implicit return, with flow-sensitive local
        // narrowing applied for `isa`-guarded conditions (Issue #5181).
        let narrow_restore = self.apply_then_narrowings(condition);
        self.compile_block_with_implicit_return(then_branch, return_type.clone())?;
        self.restore_then_narrowings(narrow_restore);

        // If there's an else branch, we need to jump over it after then-branch
        // (But since then-branch ends with a return, this jump is actually unreachable)
        // However, we still need the else label for the JumpIfZero
        let else_start = self.here();
        for patch_pos in condition_false_jumps {
            self.patch_jump(patch_pos, else_start);
        }

        // Compile else-branch with implicit return. For two-member unions, an
        // `isa` guard can prove the negated branch has the remaining concrete
        // type, giving the first codegen-connected union-split path (Issue #5077).
        if let Some(else_block) = else_branch {
            let else_restore = self.apply_else_narrowings(condition);
            self.compile_block_with_implicit_return(else_block, return_type)?;
            self.restore_then_narrowings(else_restore);
        } else {
            // No else branch - return default value
            self.emit_default_return(return_type);
        }

        Ok(())
    }

    /// Compile a block with implicit return (the last statement returns its value).
    fn compile_block_with_implicit_return(
        &mut self,
        block: &Block,
        return_type: ValueType,
    ) -> CResult<()> {
        let stmts = &block.stmts;

        if stmts.is_empty() {
            // Empty block - return default value
            self.emit_default_return(return_type);
            return Ok(());
        }

        // Compile all statements except the last one normally
        for stmt in &stmts[..stmts.len() - 1] {
            self.compile_stmt(stmt)?;
        }

        // Handle the last statement - it determines the return value
        let last_stmt = &stmts[stmts.len() - 1];
        match last_stmt {
            Stmt::Return {
                value: Some(expr), ..
            } => {
                let ty = self.compile_expr(expr)?;
                if should_return_as_expected_type(&ty, &return_type) {
                    self.emit_return_for_type(return_type);
                } else {
                    self.emit_return_for_type(ty);
                }
            }
            Stmt::Return { value: None, .. } => {
                self.emit(Instr::ReturnNothing);
            }
            Stmt::Expr { expr, .. } => {
                let actual_ty = self.compile_expr(expr)?;
                if actual_ty != return_type
                    && can_convert_type(actual_ty.clone(), return_type.clone())
                {
                    self.emit_type_conversion(actual_ty, return_type.clone());
                    self.emit_return_for_type(return_type);
                } else if should_return_as_expected_type(&actual_ty, &return_type) {
                    self.emit_return_for_type(return_type);
                } else {
                    self.emit_return_for_type(actual_ty);
                }
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                // Nested if - recursively handle
                self.compile_if_with_implicit_return(
                    condition,
                    then_branch,
                    else_branch.as_ref(),
                    return_type,
                )?;
            }
            Stmt::Try { .. } => {
                // Tail-position `try/catch[/else/finally]` returns the executed
                // branch's value rather than the type default (Issue #6223).
                self.compile_try_with_implicit_return(last_stmt, return_type)?;
            }
            Stmt::Block(block) => {
                self.compile_block_with_implicit_return(block, return_type)?;
            }
            Stmt::FunctionDef { func, .. } => {
                self.compile_stmt(last_stmt)?;
                self.emit(Instr::LoadAny(func.name.clone()));
                self.emit(Instr::ReturnAny);
            }
            _ => {
                // Other statements - compile normally and return default
                self.compile_stmt(last_stmt)?;
                self.emit_default_return(return_type);
            }
        }

        Ok(())
    }

    /// Compile a tail-position `try/catch[/else/finally]` as an implicit
    /// return. The `Stmt::Try` is converted into the same value-producing
    /// `Expr::LetBlock` form used in expression position (Issue #4784), so the
    /// returned value is the last expression of whichever branch executed
    /// instead of the return type's default (Issue #6223). Falls back to plain
    /// statement compilation + default return when the conversion fails (only
    /// possible for a non-`Try` statement, which never reaches here).
    fn compile_try_with_implicit_return(
        &mut self,
        stmt: &Stmt,
        return_type: ValueType,
    ) -> CResult<()> {
        let span = stmt.span();
        match crate::lowering::expr::try_stmt_into_value_expr(stmt.clone(), span) {
            Some(expr) => {
                let actual_ty = self.compile_expr(&expr)?;
                if actual_ty != return_type
                    && can_convert_type(actual_ty.clone(), return_type.clone())
                {
                    self.emit_type_conversion(actual_ty, return_type.clone());
                    self.emit_return_for_type(return_type);
                } else if should_return_as_expected_type(&actual_ty, &return_type) {
                    self.emit_return_for_type(return_type);
                } else {
                    self.emit_return_for_type(actual_ty);
                }
                Ok(())
            }
            None => {
                self.compile_stmt(stmt)?;
                self.emit_default_return(return_type);
                Ok(())
            }
        }
    }

    pub(super) fn compile_stmt(&mut self, stmt: &Stmt) -> CResult<()> {
        if self.compile_try_stmt(stmt)?.is_some() {
            return Ok(());
        }

        match stmt {
            Stmt::Block(block) => {
                // Inline block: compile all statements in the block
                self.compile_block(block)?;
                Ok(())
            }
            Stmt::Assign { var, value, .. } => {
                if self.const_bindings.contains(var)
                    && !self.pending_const_bindings.remove(var)
                    && !self.strict_undefined_check
                    && self.local_scope_depth == 0
                {
                    self.emit(Instr::PushStr(format!(
                        "invalid assignment to constant Main.{}",
                        var
                    )));
                    self.emit(Instr::ThrowError);
                    return Ok(());
                }
                let was_pending_const = self.pending_const_bindings.remove(var);
                let folded_const_value = if was_pending_const
                    && !self.strict_undefined_check
                    && self.local_scope_depth == 0
                {
                    crate::compile::const_prop::fold_expr_const_value(value, &|name| {
                        self.const_values.get(name).cloned()
                    })
                } else {
                    None
                };
                // Check for module assignment: S = Statistics, R = Random, etc.
                // Also handle transitive aliases: T = S where S is already a module alias
                if let Expr::Var(module_name, _) = value {
                    // Check if it's a known stdlib module
                    if is_stdlib_module(module_name) {
                        self.module_aliases.insert(var.clone(), module_name.clone());
                        self.locals.insert(var.clone(), ValueType::Module);
                        return Ok(());
                    }
                    // Check if it's an existing module alias (transitive alias)
                    if let Some(resolved) = self.module_aliases.get(module_name).cloned() {
                        self.module_aliases.insert(var.clone(), resolved);
                        self.locals.insert(var.clone(), ValueType::Module);
                        return Ok(());
                    }
                    // Check if it's a user-defined module (e.g. `const MA = Mod1`,
                    // Issue #8114). Binding a module to a `const`/variable makes the
                    // binding an alias for that module, so `MA.member` must resolve
                    // the member inside `Mod1` instead of being treated as struct
                    // field access on a `Module` value (which raised
                    // "GetFieldByName: expected struct, got Module").
                    if self.module_functions.contains_key(module_name)
                        || self.module_exports.contains_key(module_name)
                    {
                        self.module_aliases
                            .insert(var.clone(), module_name.clone());
                        self.locals.insert(var.clone(), ValueType::Module);
                        return Ok(());
                    }
                }

                let inferred_julia_type = self.infer_julia_type(value);

                // Check if there's a pre-populated "wider" type for this variable
                // This ensures consistent type usage when a variable starts as I64
                // but later receives F64 values (e.g., sum = 0; sum = sum + f64_val)
                let target_ty = self.locals.get(var).cloned();
                let ty = self.compile_expr(value)?;

                // Check if this is a compound assignment pattern (var = var op mixed_type_var)
                // where the operand is a variable in mixed_type_vars.
                // This only applies when we know the operand is from a mixed I64/F64 variable,
                // NOT when it's an untyped parameter (which could be any type at runtime).
                let is_mixed_type_compound_assignment = match value {
                    Expr::BinaryOp { left, right, .. } => {
                        let is_left_var =
                            matches!(left.as_ref(), Expr::Var(name, _) if name == var);
                        let right_is_mixed = matches!(right.as_ref(), Expr::Var(name, _) if self.mixed_type_vars.contains(name));
                        is_left_var && right_is_mixed
                    }
                    _ => false,
                };

                if ty == ValueType::Function {
                    if let Some(alias_target) = self.resolve_function_alias_value(value) {
                        self.function_aliases.insert(var.clone(), alias_target);
                    } else {
                        self.function_aliases.remove(var);
                    }
                } else {
                    self.function_aliases.remove(var);
                }

                let final_ty = match (target_ty, ty.clone()) {
                    // If target is Any AND it's a function parameter with no type annotation,
                    // keep it as Any to use StoreAny/LoadAny for dynamic type handling.
                    (Some(ValueType::Any), _) if self.any_params.contains(var) => ValueType::Any,
                    // If target is Any AND it's a mixed-type variable (F64+I64 in different branches),
                    // use dynamic typing to allow runtime type changes (Julia semantics).
                    (Some(ValueType::Any), ValueType::I64)
                    | (Some(ValueType::Any), ValueType::F64)
                        if self.mixed_type_vars.contains(var) =>
                    {
                        ValueType::Any
                    }
                    // Issue #3535/#3536: target Any AND mixed_type_vars contains var
                    // because of incompatible non-numeric reassignment (e.g. Int64
                    // and String, or Struct and Nothing). Keep the slot dynamic so
                    // every assignment compiles to StoreAny.
                    (Some(ValueType::Any), _) if self.mixed_type_vars.contains(var) => {
                        ValueType::Any
                    }
                    (Some(target), incoming)
                        if self.mixed_type_vars.contains(var)
                            && !static_assignment_types_compatible(&target, &incoming) =>
                    {
                        ValueType::Any
                    }
                    // For mixed-type variables (F64+I64 in sequence), use dynamic typing.
                    // This allows `x = 1.0; x = 2` to have typeof(x) == Int64, not Float64.
                    (Some(ValueType::F64), ValueType::I64)
                        if self.mixed_type_vars.contains(var) =>
                    {
                        // Use the actual type (I64) for proper dynamic typing
                        ty
                    }
                    (Some(ValueType::I64), ValueType::F64)
                        if self.mixed_type_vars.contains(var) =>
                    {
                        // Use the actual type (F64) for proper dynamic typing
                        ty
                    }
                    // If pre-populated type is F64 but compiled type is I64, convert.
                    // This is needed for widening where the type inference determined
                    // that a variable can be both F64 and I64 (e.g., in control flow).
                    // Only applies to non-mixed-type variables (checked above).
                    (Some(ValueType::F64), ValueType::I64) => {
                        self.emit(Instr::ToF64);
                        ValueType::F64
                    }
                    // Compound assignments (x = x op y) where y is a mixed-type variable:
                    // Preserve x's numeric type because y will be numeric at runtime.
                    // This does NOT apply when y is an untyped parameter (could be any type).
                    (Some(ValueType::I64), ValueType::Any) if is_mixed_type_compound_assignment => {
                        self.emit(Instr::DynamicToI64);
                        ValueType::I64
                    }
                    (Some(ValueType::F64), ValueType::Any) if is_mixed_type_compound_assignment => {
                        self.emit(Instr::DynamicToF64);
                        ValueType::F64
                    }
                    // If pre-populated type is Struct but compiled type is Any,
                    // preserve the struct type (compile_binary_op may return Any
                    // for dynamic dispatch but type inference correctly identified the type)
                    (Some(ValueType::Struct(type_id)), ValueType::Any) => {
                        ValueType::Struct(type_id)
                    }
                    // Issue #4827: pre-inference (collect_local_types via
                    // infer_value_type) maps `IOBuffer()` -> IO, but the
                    // compile-time `compile_expr` for the constructor can return
                    // Any when the call is routed through generic base-function
                    // dispatch rather than the IO builtin arm. Preserve the IO
                    // slot type so `infer_expr_type(buf)` reports IO at later
                    // `print(buf, …)` / `println(buf, …)` call sites, enabling the
                    // statically-IO multi-arg user-`show` split (and matching the
                    // global_types IO routing established by Issue #5035). Without
                    // this, the `_ => ty` fallback overwrote the IO slot with Any,
                    // so multi-arg `print(buf, a, x, b)` field-dumped the struct.
                    (Some(ValueType::IO), ValueType::Any) => ValueType::IO,
                    // Note: Complex type conversions are now handled via Pure Julia convert().
                    // Otherwise, use the compiled type.
                    _ => ty,
                };

                if let Some(type_value) = self.resolve_static_datatype_value(value) {
                    self.type_value_aliases.insert(var.clone(), type_value);
                } else {
                    self.type_value_aliases.remove(var);
                }

                // Track JuliaType for parametric types to enable proper dispatch.
                //
                // DESIGN PRINCIPLE: Track based on *inferred type*, not *expression form*.
                // This ensures all sources of parametric types are covered: literals,
                // variable reassignment (t2 = t1), function returns (t3 = make_pair()),
                // conditional expressions (t = if c; (1,2) else (3,4) end), etc.
                //
                // Non-parametric ValueTypes (Tuple, Array) cannot distinguish between
                // Tuple{Int64, Int64} and Tuple{String, Float64}, or Vector{Int64} and
                // Vector{Any}. We store the full JuliaType in `julia_type_locals` so
                // that `infer_julia_type()` can recover the parametric type for method
                // dispatch.
                //
                // If the new assignment does not prove a precise JuliaType, remove any
                // previous precise entry. Otherwise a reused variable such as
                // `arr = [1]; arr = Int8[1]` keeps dispatching as `Vector{Int64}`
                // even though `locals` has moved on to the current array element type.
                //
                // See Issue #1748 (original), #2305 (reassignment), #2319 (conditional),
                // #2352 (VectorOf/MatrixOf dispatch), #5588 (stale reassignment).
                let track_julia_type = matches!(
                    inferred_julia_type,
                    JuliaType::TupleOf(_) | JuliaType::VectorOf(_) | JuliaType::MatrixOf(_)
                ) || matches!(&inferred_julia_type, JuliaType::Struct(name) if name.starts_with("@NamedTuple{") || is_dict_struct_name(name));
                if track_julia_type {
                    self.julia_type_locals
                        .insert(var.clone(), inferred_julia_type);
                } else {
                    self.julia_type_locals.remove(var);
                }

                self.store_local(var, final_ty);
                if was_pending_const && !self.strict_undefined_check && self.local_scope_depth == 0
                {
                    self.const_bindings.insert(var.clone());
                    if let Some(value) = folded_const_value {
                        self.const_values.insert(var.clone(), value);
                    } else {
                        self.const_values.remove(var);
                    }
                } else if !self.const_bindings.contains(var) {
                    self.const_values.remove(var);
                }
                Ok(())
            }
            Stmt::AddAssign { var, value, .. } => {
                let var_ty = self.locals.get(var).cloned().unwrap_or(ValueType::I64);
                self.load_local(var)?;
                self.compile_expr_as(value, var_ty.clone())?;
                self.emit(match var_ty {
                    ValueType::I64 => Instr::AddI64,
                    ValueType::F64 => Instr::AddF64,
                    _ => return err("AddAssign not supported for this type"),
                });
                self.store_local(var, var_ty);
                Ok(())
            }
            Stmt::For {
                var,
                start,
                end,
                step,
                body,
                span,
            } => {
                // Issue #3550: when start/end are typed non-Int64 integers (e.g.
                // `UInt8(1):UInt8(3)`), the optimized I64-specialized path drops
                // the element type. Rewrite the loop to the generic `ForEach`
                // path (with a lazy `Range` value) so iteration produces values
                // of the right type. The default `Int64` case continues using
                // the fast path below.
                let start_ty = self.infer_expr_type(start);
                let end_ty = self.infer_expr_type(end);
                let step_ty = step.as_ref().map(|s| self.infer_expr_type(s));
                // A start/end/step whose inferred type is a non-integer
                // (float / BigFloat) must also divert to the generic ForEach
                // path: the I64 fast path below pins every component to
                // `ValueType::I64`, so a Float-typed step that is not a bare
                // float *literal* — e.g. `0:(2π/12):2π`, where `2π/12` is a
                // `BinaryOp` and so escapes the lowering-time literal check in
                // control_for.rs — gets truncated to 0 and the loop iterates
                // zero times (Issue #7800, follow-up to #3551). `infer_expr_type`
                // resolves `π`, arithmetic, etc., so it catches computed float
                // bounds that the lowering literal heuristic cannot.
                let needs_typed_range = matches!(
                    start_ty,
                    ValueType::I8
                        | ValueType::I16
                        | ValueType::I32
                        | ValueType::U8
                        | ValueType::U16
                        | ValueType::U32
                        | ValueType::U64
                        | ValueType::Char
                        | ValueType::F64
                        | ValueType::F32
                        | ValueType::F16
                        | ValueType::BigFloat
                ) || matches!(
                    end_ty,
                    ValueType::I8
                        | ValueType::I16
                        | ValueType::I32
                        | ValueType::U8
                        | ValueType::U16
                        | ValueType::U32
                        | ValueType::U64
                        | ValueType::Char
                        | ValueType::F64
                        | ValueType::F32
                        | ValueType::F16
                        | ValueType::BigFloat
                ) || matches!(
                    step_ty,
                    Some(ValueType::I8)
                        | Some(ValueType::I16)
                        | Some(ValueType::I32)
                        | Some(ValueType::U8)
                        | Some(ValueType::U16)
                        | Some(ValueType::U32)
                        | Some(ValueType::U64)
                        | Some(ValueType::F64)
                        | Some(ValueType::F32)
                        | Some(ValueType::F16)
                        | Some(ValueType::BigFloat)
                );
                // Char ranges (`for c in 'a':'c'`) take the same generic
                // ForEach path as small-int ranges — the I64 fast path
                // below would store the loop var as Int64 codepoint,
                // bypassing `RangeValue::typed_element` which exists for
                // exactly this purpose (Issue #4796, follow-up to #4795).
                if needs_typed_range {
                    let range_expr = Expr::Range {
                        start: Box::new(start.clone()),
                        step: step.clone().map(Box::new),
                        stop: Box::new(end.clone()),
                        span: *span,
                    };
                    let foreach = Stmt::ForEach {
                        var: var.clone(),
                        iterable: range_expr,
                        body: body.clone(),
                        span: *span,
                    };
                    return self.compile_stmt(&foreach);
                }

                // For loop: for var in start:end or start:step:end
                self.locals.insert(var.clone(), ValueType::I64);

                // Issue #5166: when the step is a compile-time constant, the per-
                // iteration sign check is redundant — the loop can only ever count in
                // one direction. Detect a constant non-zero step and emit a single-
                // direction exit test plus a constant increment. A constant step of
                // zero falls back to the dynamic path so its (pre-existing) behavior
                // is unchanged.
                if let Some(const_step) = const_int_step(step).filter(|k| *k != 0) {
                    return self
                        .compile_const_step_for(var, start, end, const_step, body);
                }

                let stop_var = self.new_temp("stop");
                let step_var = self.new_temp("step");

                // Compile and store stop value
                self.compile_expr_as(end, ValueType::I64)?;
                self.emit(Instr::StoreI64(stop_var.clone()));

                // Compile and store step value (default 1 if not specified)
                if let Some(step_expr) = step {
                    self.compile_expr_as(step_expr, ValueType::I64)?;
                } else {
                    self.emit(Instr::PushI64(1));
                }
                self.emit(Instr::StoreI64(step_var.clone()));

                // Initialize loop variable
                self.compile_expr_as(start, ValueType::I64)?;
                self.emit(Instr::StoreI64(var.clone()));

                let loop_start = self.here();

                // Push loop context for break/continue
                let mut loop_ctx = LoopContext {
                    exit_patches: Vec::new(),
                    continue_patches: Vec::new(),
                };

                // Check loop condition based on step sign:
                // If step > 0: continue while var <= stop (exit when var > stop)
                // If step < 0: continue while var >= stop (exit when var < stop)
                // We check: (step > 0 && var > stop) || (step < 0 && var < stop)

                // Check if step > 0
                self.emit(Instr::LoadI64(step_var.clone()));
                self.emit(Instr::PushI64(0));
                self.emit(Instr::GtI64);
                let j_positive = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX)); // jump to negative check if step <= 0

                // Step is positive: check var > stop
                self.emit(Instr::LoadI64(var.clone()));
                self.emit(Instr::LoadI64(stop_var.clone()));
                self.emit(Instr::GtI64);
                let j_exit_pos = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX)); // continue if var <= stop
                let j_to_exit1 = self.here();
                self.emit(Instr::Jump(usize::MAX)); // exit loop
                loop_ctx.exit_patches.push(j_to_exit1);

                // Step is negative: check var < stop
                let negative_check = self.here();
                self.patch_jump(j_positive, negative_check);
                self.emit(Instr::LoadI64(var.clone()));
                self.emit(Instr::LoadI64(stop_var.clone()));
                self.emit(Instr::LtI64);
                let j_exit_neg = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX)); // continue if var >= stop
                let j_to_exit2 = self.here();
                self.emit(Instr::Jump(usize::MAX)); // exit loop
                loop_ctx.exit_patches.push(j_to_exit2);

                let body_start = self.here();
                self.patch_jump(j_exit_pos, body_start);
                self.patch_jump(j_exit_neg, body_start);

                // Compile body with loop context
                self.loop_stack.push(loop_ctx);
                self.compile_block(body)?;
                let loop_ctx = self.loop_stack.pop().unwrap();

                let continue_target = self.here();

                // Increment by step
                self.emit(Instr::LoadI64(var.clone()));
                self.emit(Instr::LoadI64(step_var.clone()));
                self.emit(Instr::AddI64);
                self.emit(Instr::StoreI64(var.clone()));

                self.emit(Instr::Jump(loop_start));

                let exit = self.here();
                // Patch all exit jumps (from condition and any break statements)
                for patch_pos in loop_ctx.exit_patches {
                    self.patch_jump(patch_pos, exit);
                }
                for patch_pos in loop_ctx.continue_patches {
                    self.patch_jump(patch_pos, continue_target);
                }

                Ok(())
            }
            Stmt::ForEach {
                var,
                iterable,
                body,
                ..
            } => {
                // ForEach loop: for var in iterable
                // Strategy:
                // 1. Compile and store iterable
                // 2. Call iterate(collection) to get (element, state) or Nothing
                // 3. If Nothing, exit loop
                // 4. Store element in loop variable, execute body
                // 5. Call iterate(collection, state) to get next (element, state) or Nothing
                // 6. If Nothing, exit; otherwise loop back to step 4
                //
                // For custom iterators (struct types), we use Pure Julia iterate methods.
                // For builtin types (Array, Range, Tuple, String), we use VM instructions.

                // Check if we should use Pure Julia iterate (for struct types)
                let iterable_ty = self.infer_julia_type(iterable);
                let use_pure_julia_iterate = self.should_use_pure_julia_iterate(&iterable_ty);

                // Issue #5168: for the builtin (non pure-Julia) iterate path the
                // VM can produce `(element, state)` split across the stack instead
                // of allocating a `(element, state)` tuple every iteration. The
                // pure-Julia path keeps the tuple-based lowering below because its
                // `iterate` methods return real tuples (and may suspend frames).
                if !use_pure_julia_iterate {
                    return self.compile_foreach_split(var, iterable, body);
                }

                // Store the iterable
                let iterable_var = self.new_temp("iterable");
                let state_var = self.new_temp("state");
                let iter_result_var = self.new_temp("iter_result");
                self.compile_expr(iterable)?;
                self.emit(Instr::StoreAny(iterable_var.clone()));

                // Get first iteration result: iterate(collection)
                self.emit(Instr::LoadAny(iterable_var.clone()));
                self.emit_iterate_call_1(&iterable_ty)?;
                // Stack: (element, state) or Nothing
                self.emit(Instr::StoreAny(iter_result_var.clone()));

                // Check if Nothing
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::IsNothing);
                let j_exit_first = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX)); // Continue if NOT Nothing
                let j_to_exit_first = self.here();
                self.emit(Instr::Jump(usize::MAX)); // Exit if Nothing

                let continue_after_check = self.here();
                self.patch_jump(j_exit_first, continue_after_check);

                // Extract element and state from tuple
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::TupleSecond); // Get state
                self.emit(Instr::StoreAny(state_var.clone()));
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::TupleFirst); // Get element

                let loop_start = self.here();

                // Store element in loop variable
                self.emit(Instr::StoreAny(var.clone()));
                self.locals.insert(var.clone(), ValueType::Any);

                // Push loop context for break/continue
                let loop_ctx = LoopContext {
                    exit_patches: vec![j_to_exit_first],
                    continue_patches: Vec::new(),
                };

                // Compile body with loop context
                let inbounds_array_var = proven_inbounds_loop_array_var(iterable);
                if let Some(array_var) = inbounds_array_var {
                    self.push_proven_inbounds_index(array_var, var);
                }
                self.loop_stack.push(loop_ctx);
                let body_result = self.compile_block(body);
                let loop_ctx = self.loop_stack.pop().unwrap();
                if inbounds_array_var.is_some() {
                    self.pop_proven_inbounds_index();
                }
                body_result?;

                let continue_target = self.here();

                // Get next iteration result: iterate(collection, state)
                self.emit(Instr::LoadAny(iterable_var.clone()));
                self.emit(Instr::LoadAny(state_var.clone()));
                self.emit_iterate_call_2(&iterable_ty)?;
                // Stack: (element, state) or Nothing
                self.emit(Instr::StoreAny(iter_result_var.clone()));

                // Check if Nothing
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::IsNothing);
                let j_check_loop = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX)); // Continue if NOT Nothing
                let j_to_exit_loop = self.here();
                self.emit(Instr::Jump(usize::MAX)); // Exit if Nothing

                let continue_after_check2 = self.here();
                self.patch_jump(j_check_loop, continue_after_check2);

                // Extract element and state from tuple
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::TupleSecond); // Get state
                self.emit(Instr::StoreAny(state_var.clone()));
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::TupleFirst); // Get element

                self.emit(Instr::Jump(loop_start));

                let exit = self.here();

                // Patch all exit jumps
                self.patch_jump(j_to_exit_first, exit);
                self.patch_jump(j_to_exit_loop, exit);
                for patch_pos in loop_ctx.exit_patches {
                    if patch_pos != j_to_exit_first {
                        self.patch_jump(patch_pos, exit);
                    }
                }
                for patch_pos in loop_ctx.continue_patches {
                    self.patch_jump(patch_pos, continue_target);
                }

                Ok(())
            }
            Stmt::ForEachTuple {
                vars,
                iterable,
                body,
                ..
            } => {
                // ForEachTuple loop: for (a, b) in iterable
                // Similar to ForEach but destructures each element into multiple vars
                //
                // For custom iterators (struct types), we use Pure Julia iterate methods.
                // For builtin types (Array, Range, Tuple, String), we use VM instructions.

                // Check if we should use Pure Julia iterate (for struct types)
                let iterable_ty = self.infer_julia_type(iterable);
                let use_pure_julia_iterate = self.should_use_pure_julia_iterate(&iterable_ty);

                let iterable_var = self.new_temp("iterable");
                let state_var = self.new_temp("state");
                let iter_result_var = self.new_temp("iter_result");
                let elem_var = self.new_temp("elem");
                self.compile_expr(iterable)?;
                self.emit(Instr::StoreAny(iterable_var.clone()));

                // Get first iteration result: iterate(collection)
                self.emit(Instr::LoadAny(iterable_var.clone()));
                if use_pure_julia_iterate {
                    self.emit_iterate_call_1(&iterable_ty)?;
                } else {
                    self.emit(Instr::IterateFirst);
                }
                self.emit(Instr::StoreAny(iter_result_var.clone()));

                // Check if Nothing
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::IsNothing);
                let j_exit_first = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX));
                let j_to_exit_first = self.here();
                self.emit(Instr::Jump(usize::MAX));

                let continue_after_check = self.here();
                self.patch_jump(j_exit_first, continue_after_check);

                // Extract element and state from tuple
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::TupleSecond);
                self.emit(Instr::StoreAny(state_var.clone()));
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::TupleFirst);
                self.emit(Instr::StoreAny(elem_var.clone()));

                let loop_start = self.here();

                // Destructure element tuple into individual variables
                // Element is already a tuple like (1, 10), extract each component
                for (i, var) in vars.iter().enumerate() {
                    self.emit(Instr::LoadAny(elem_var.clone()));
                    self.emit(Instr::PushI64((i + 1) as i64)); // 1-indexed
                    self.emit(Instr::TupleGet);
                    self.emit(Instr::StoreAny(var.clone()));
                    self.locals.insert(var.clone(), ValueType::Any);
                }

                // Push loop context for break/continue
                let loop_ctx = LoopContext {
                    exit_patches: vec![j_to_exit_first],
                    continue_patches: Vec::new(),
                };

                // Compile body with loop context
                self.loop_stack.push(loop_ctx);
                self.compile_block(body)?;
                let loop_ctx = self.loop_stack.pop().unwrap();

                let continue_target = self.here();

                // Get next iteration result: iterate(collection, state)
                self.emit(Instr::LoadAny(iterable_var.clone()));
                self.emit(Instr::LoadAny(state_var.clone()));
                if use_pure_julia_iterate {
                    self.emit_iterate_call_2(&iterable_ty)?;
                } else {
                    self.emit(Instr::IterateNext);
                }
                self.emit(Instr::StoreAny(iter_result_var.clone()));

                // Check if Nothing
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::IsNothing);
                let j_check_loop = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX));
                let j_to_exit_loop = self.here();
                self.emit(Instr::Jump(usize::MAX));

                let continue_after_check2 = self.here();
                self.patch_jump(j_check_loop, continue_after_check2);

                // Extract element and state from tuple
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::TupleSecond);
                self.emit(Instr::StoreAny(state_var.clone()));
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::TupleFirst);
                self.emit(Instr::StoreAny(elem_var.clone()));

                self.emit(Instr::Jump(loop_start));

                let exit = self.here();

                // Patch all exit jumps
                self.patch_jump(j_to_exit_first, exit);
                self.patch_jump(j_to_exit_loop, exit);
                for patch_pos in loop_ctx.exit_patches {
                    if patch_pos != j_to_exit_first {
                        self.patch_jump(patch_pos, exit);
                    }
                }
                for patch_pos in loop_ctx.continue_patches {
                    self.patch_jump(patch_pos, continue_target);
                }

                Ok(())
            }
            Stmt::While {
                condition, body, ..
            } => {
                let loop_start = self.here();

                // Push loop context for break/continue
                let mut loop_ctx = LoopContext {
                    exit_patches: Vec::new(),
                    continue_patches: Vec::new(),
                };

                // Compile condition in branch context so `&&` / `||` do not
                // materialize a stack Bool before the loop-exit branch.
                loop_ctx
                    .exit_patches
                    .extend(self.compile_condition_false_jumps(condition)?);

                // Compile body with loop context
                self.loop_stack.push(loop_ctx);
                let narrow_restore = self.apply_then_narrowings(condition);
                self.compile_block(body)?;
                self.restore_then_narrowings(narrow_restore);
                let loop_ctx = self.loop_stack.pop().unwrap();

                self.emit(Instr::Jump(loop_start));

                let exit = self.here();
                // Patch all exit jumps (from condition and any break statements)
                for patch_pos in loop_ctx.exit_patches {
                    self.patch_jump(patch_pos, exit);
                }
                for patch_pos in loop_ctx.continue_patches {
                    self.patch_jump(patch_pos, loop_start);
                }
                Ok(())
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                // Dead code elimination: skip provably dead branches.
                // Fires on a bare Bool literal (Issue #3364) and, via the
                // const-bool folder, on any pure const-foldable condition such
                // as `if 1 < 2` or `if true && false` (Issue #5182).
                if let Some(b) = const_bool_condition_with_lookup(condition, &|name| {
                    self.const_values.get(name).cloned()
                }) {
                    if b {
                        // Condition is always true: only compile then-branch
                        self.compile_block(then_branch)?;
                    } else if let Some(else_block) = else_branch {
                        // Condition is always false: only compile else-branch
                        self.compile_block(else_block)?;
                    }
                    return Ok(());
                }

                // Compile condition in branch context so `&&` / `||` do not
                // materialize a stack Bool before the else-branch jump.
                let condition_false_jumps = self.compile_condition_false_jumps(condition)?;

                // Flow-sensitive local narrowing for `isa`-guarded then-branch
                // (Issue #5181): refine `self.locals` only while compiling the
                // then-branch, then restore so the else-branch / fall-through is
                // unaffected.
                let narrow_restore = self.apply_then_narrowings(condition);
                self.compile_block(then_branch)?;
                self.restore_then_narrowings(narrow_restore);
                let j_end = self.here();
                self.emit(Instr::Jump(usize::MAX));

                let else_start = self.here();
                for patch_pos in condition_false_jumps {
                    self.patch_jump(patch_pos, else_start);
                }

                if let Some(else_block) = else_branch {
                    let else_restore = self.apply_else_narrowings(condition);
                    self.compile_block(else_block)?;
                    self.restore_then_narrowings(else_restore);
                }

                let end = self.here();
                self.patch_jump(j_end, end);
                Ok(())
            }
            Stmt::Return { value, .. } => {
                // Check if there are pending finally blocks
                if self.finally_stack.is_empty() {
                    // No finally blocks - original behavior
                    if let Some(expr) = value {
                        let ty = self.compile_expr(expr)?;
                        self.emit(match ty {
                            ValueType::I64 => Instr::ReturnI64,
                            ValueType::F64 => Instr::ReturnF64,
                            ValueType::Array | ValueType::ArrayOf(_, _) => Instr::ReturnArray,
                            ValueType::Str => Instr::ReturnAny,
                            // Use ReturnAny for Nothing to consume the pushed value (Issue #2072)
                            ValueType::Nothing => Instr::ReturnAny,
                            ValueType::Missing => Instr::ReturnAny,
                            ValueType::Struct(_)
                            | ValueType::ComplexF32
                            | ValueType::ComplexF64 => Instr::ReturnStruct,
                            ValueType::Rng => Instr::ReturnRng,
                            ValueType::Range => Instr::ReturnRange,
                            ValueType::Tuple => Instr::ReturnTuple,
                            ValueType::NamedTuple => Instr::ReturnNamedTuple,
                            ValueType::Dict | ValueType::Set => Instr::ReturnDict,
                            ValueType::Generator => Instr::ReturnAny,
                            ValueType::Char => Instr::ReturnAny,
                            ValueType::Any => Instr::ReturnAny,
                            ValueType::DataType => Instr::ReturnAny,
                            ValueType::Module => Instr::ReturnAny,
                            ValueType::BigInt => Instr::ReturnAny,
                            ValueType::BigFloat => Instr::ReturnAny,
                            ValueType::IO => Instr::ReturnAny,
                            ValueType::Function => Instr::ReturnAny,
                            ValueType::I8 | ValueType::I16 | ValueType::I32 | ValueType::I128 => {
                                Instr::ReturnAny
                            }
                            ValueType::U8
                            | ValueType::U16
                            | ValueType::U32
                            | ValueType::U64
                            | ValueType::U128 => Instr::ReturnAny,
                            ValueType::F32 => Instr::ReturnF32,
                            ValueType::F16 => Instr::ReturnF16,
                            ValueType::Bool => Instr::ReturnAny,
                            ValueType::Symbol
                            | ValueType::Expr
                            | ValueType::QuoteNode
                            | ValueType::LineNumberNode
                            | ValueType::GlobalRef => Instr::ReturnAny,
                            ValueType::Pairs => Instr::ReturnAny,
                            ValueType::Regex | ValueType::RegexMatch => Instr::ReturnAny,
                            ValueType::Enum => Instr::ReturnAny,
                            ValueType::Union(_) => Instr::ReturnAny,
                            ValueType::Memory | ValueType::MemoryOf(_) => Instr::ReturnAny,
                        });
                    } else {
                        self.emit(Instr::ReturnNothing);
                    }
                } else {
                    // Has finally blocks - save return value, execute finally, then return
                    let (saved_temp, saved_ty) = if let Some(expr) = value {
                        let ty = self.compile_expr(expr)?;
                        let temp = self.new_temp("return_val");
                        match ty {
                            ValueType::I64 => self.emit(Instr::StoreI64(temp.clone())),
                            ValueType::F64 => self.emit(Instr::StoreF64(temp.clone())),
                            ValueType::Array | ValueType::ArrayOf(_, _) => {
                                self.emit(Instr::StoreArray(temp.clone()))
                            }
                            ValueType::Tuple => self.emit(Instr::StoreTuple(temp.clone())),
                            ValueType::NamedTuple => {
                                self.emit(Instr::StoreNamedTuple(temp.clone()))
                            }
                            ValueType::Dict | ValueType::Set => {
                                self.emit(Instr::StoreDict(temp.clone()))
                            }
                            ValueType::Range => self.emit(Instr::StoreRange(temp.clone())),
                            ValueType::Rng => self.emit(Instr::StoreRng(temp.clone())),
                            ValueType::Struct(_) => self.emit(Instr::StoreStruct(temp.clone())),
                            _ => self.emit(Instr::StoreAny(temp.clone())),
                        }
                        self.locals.insert(temp.clone(), ty.clone());
                        (Some(temp), ty)
                    } else {
                        (None, ValueType::Nothing)
                    };

                    // Execute all pending finally blocks in reverse order
                    let finally_blocks: Vec<_> = self
                        .finally_stack
                        .iter()
                        .map(|ctx| ctx.finally_block.clone())
                        .collect();
                    for block in finally_blocks.iter().rev() {
                        self.compile_block(block)?;
                    }

                    // Load return value and return
                    if let Some(ref temp) = saved_temp {
                        match saved_ty {
                            ValueType::I64 => self.emit(Instr::LoadI64(temp.clone())),
                            ValueType::F64 => self.emit(Instr::LoadF64(temp.clone())),
                            ValueType::Array | ValueType::ArrayOf(_, _) => {
                                self.emit(Instr::LoadArray(temp.clone()))
                            }
                            ValueType::Tuple => self.emit(Instr::LoadTuple(temp.clone())),
                            ValueType::NamedTuple => self.emit(Instr::LoadNamedTuple(temp.clone())),
                            ValueType::Dict | ValueType::Set => {
                                self.emit(Instr::LoadDict(temp.clone()))
                            }
                            ValueType::Range => self.emit(Instr::LoadRange(temp.clone())),
                            ValueType::Rng => self.emit(Instr::LoadRng(temp.clone())),
                            ValueType::Struct(_) => self.emit(Instr::LoadStruct(temp.clone())),
                            _ => self.emit(Instr::LoadAny(temp.clone())),
                        }
                    }
                    self.emit(match saved_ty {
                        ValueType::I64 => Instr::ReturnI64,
                        ValueType::F64 => Instr::ReturnF64,
                        ValueType::Array | ValueType::ArrayOf(_, _) => Instr::ReturnArray,
                        ValueType::Struct(_) => Instr::ReturnStruct,
                        ValueType::Rng => Instr::ReturnRng,
                        ValueType::Range => Instr::ReturnRange,
                        ValueType::Tuple => Instr::ReturnTuple,
                        ValueType::NamedTuple => Instr::ReturnNamedTuple,
                        ValueType::Dict | ValueType::Set => Instr::ReturnDict,
                        // When saved_temp is Some, a Load pushed a value — use ReturnAny
                        // to consume it. When None, no value on stack — use ReturnNothing.
                        // (Issue #2072)
                        ValueType::Nothing => {
                            if saved_temp.is_some() {
                                Instr::ReturnAny
                            } else {
                                Instr::ReturnNothing
                            }
                        }
                        _ => Instr::ReturnAny,
                    });
                }
                Ok(())
            }
            Stmt::Expr { expr, .. } => {
                if let Some(var) = const_declaration_marker(expr) {
                    if !self.strict_undefined_check && self.local_scope_depth == 0 {
                        self.pending_const_bindings.insert(var.to_string());
                    }
                    return Ok(());
                }
                let ty = self.compile_expr(expr)?;
                // Pop unused value by storing to dummy variable
                let dummy = self.new_temp("discard");
                match ty {
                    ValueType::I64 => self.emit(Instr::StoreI64(dummy)),
                    ValueType::F64 => self.emit(Instr::StoreF64(dummy)),
                    ValueType::Array | ValueType::ArrayOf(_, _) => self.emit(Instr::StoreArray(dummy)),
                    ValueType::Str => self.emit(Instr::Pop),
                    // `nothing` is a real stack value when it comes from calls like println().
                    // Discard it in statement context so it cannot sit below pending caller args.
                    ValueType::Nothing => self.emit(Instr::Pop),
                    ValueType::Missing => self.emit(Instr::Pop),
                    ValueType::Struct(_) | ValueType::ComplexF32 | ValueType::ComplexF64 => {
                        self.emit(Instr::Pop)
                    }
                    ValueType::Rng => self.emit(Instr::StoreRng(dummy)),
                    ValueType::Range => self.emit(Instr::StoreRange(dummy)),
                    ValueType::Tuple => self.emit(Instr::StoreTuple(dummy)),
                    ValueType::NamedTuple => self.emit(Instr::StoreNamedTuple(dummy)),
                    ValueType::Dict | ValueType::Set => self.emit(Instr::StoreDict(dummy)),
                    ValueType::Generator => self.emit(Instr::StoreAny(dummy)),
                    ValueType::Char => self.emit(Instr::StoreAny(dummy)),
                    ValueType::DataType => self.emit(Instr::StoreAny(dummy)),
                    ValueType::Module => self.emit(Instr::StoreAny(dummy)),
                    ValueType::Any => self.emit(Instr::StoreAny(dummy)),
                    ValueType::BigInt => self.emit(Instr::StoreAny(dummy)),
                    ValueType::BigFloat => self.emit(Instr::StoreAny(dummy)),
                    ValueType::IO => self.emit(Instr::StoreAny(dummy)),
                    ValueType::Function => self.emit(Instr::StoreAny(dummy)),
                    // Narrow integer types use StoreAny which dispatches to the NarrowInt tag.
                    // at runtime, preserving the exact Value type (e.g. I8(42), U32(99)).
                    ValueType::I8 | ValueType::I16 | ValueType::I32 | ValueType::I128 => {
                        self.emit(Instr::StoreAny(dummy))
                    }
                    ValueType::U8
                    | ValueType::U16
                    | ValueType::U32
                    | ValueType::U64
                    | ValueType::U128 => self.emit(Instr::StoreAny(dummy)),
                    ValueType::F32 => self.emit(Instr::StoreF32(dummy)),
                    ValueType::F16 => self.emit(Instr::StoreF16(dummy)),
                    ValueType::Bool => self.emit(Instr::StoreBool(dummy)),
                    // Macro system types
                    ValueType::Symbol
                    | ValueType::Expr
                    | ValueType::QuoteNode
                    | ValueType::LineNumberNode
                    | ValueType::GlobalRef => self.emit(Instr::StoreAny(dummy)),
                    // Pairs type (for kwargs...)
                    ValueType::Pairs => self.emit(Instr::StoreAny(dummy)),
                    // Regex types
                    ValueType::Regex | ValueType::RegexMatch => self.emit(Instr::StoreAny(dummy)),
                    // Enum type
                    ValueType::Enum => self.emit(Instr::StoreAny(dummy)),
                    // Union type
                    ValueType::Union(_) => self.emit(Instr::StoreAny(dummy)),
                    // Memory type
                    ValueType::Memory | ValueType::MemoryOf(_) => self.emit(Instr::StoreAny(dummy)),
                }
                Ok(())
            }
            Stmt::Meta { .. } => Ok(()),
            Stmt::Global { names, .. } => {
                // The declaration itself emits no code; it only records that the
                // named bindings are module-level for this scope. `compile_function_body`
                // already pre-scans for these, but record them here too so any
                // path that compiles statements directly stays consistent
                // (Issues #5548, #5549). At module scope the binding is already
                // global, so recording it would only widen its type to `Any` —
                // skip it there (mirrors the pre-scan guard).
                if self.strict_undefined_check || self.local_scope_depth > 0 {
                    for name in names {
                        self.declared_globals.insert(name.clone());
                    }
                }
                Ok(())
            }
            Stmt::Break { .. } => {
                // Jump to the exit of the innermost loop
                if self.loop_stack.is_empty() {
                    return err("break outside of loop");
                }
                let current_loop_depth = self.loop_stack.len();

                // Execute finally blocks inside the current loop
                let finally_blocks: Vec<_> = self
                    .finally_stack
                    .iter()
                    .filter(|ctx| ctx.loop_depth >= current_loop_depth)
                    .map(|ctx| ctx.finally_block.clone())
                    .collect();
                for block in finally_blocks.iter().rev() {
                    self.compile_block(block)?;
                }

                let j_exit = self.here();
                self.emit(Instr::Jump(usize::MAX));
                if let Some(loop_ctx) = self.loop_stack.last_mut() {
                    loop_ctx.exit_patches.push(j_exit);
                }
                Ok(())
            }
            Stmt::Continue { .. } => {
                // Jump to the entry of the innermost loop
                if self.loop_stack.is_empty() {
                    return err("continue outside of loop");
                }
                let current_loop_depth = self.loop_stack.len();

                // Execute finally blocks inside the current loop
                let finally_blocks: Vec<_> = self
                    .finally_stack
                    .iter()
                    .filter(|ctx| ctx.loop_depth >= current_loop_depth)
                    .map(|ctx| ctx.finally_block.clone())
                    .collect();
                for block in finally_blocks.iter().rev() {
                    self.compile_block(block)?;
                }

                let j_continue = self.here();
                self.emit(Instr::Jump(usize::MAX));
                if let Some(loop_ctx) = self.loop_stack.last_mut() {
                    loop_ctx.continue_patches.push(j_continue);
                }
                Ok(())
            }
            Stmt::Test {
                condition, message, ..
            } => {
                self.compile_expr_as(condition, ValueType::Bool)?;
                let msg = message.clone().unwrap_or_default();
                self.emit(Instr::Test(msg));
                Ok(())
            }
            Stmt::TestSet { name, body, .. } => {
                self.emit(Instr::TestSetBegin(name.clone()));
                let outer_locals = self.locals.clone();
                let outer_julia_type_locals = self.julia_type_locals.clone();
                let outer_mixed_type_vars = self.mixed_type_vars.clone();
                let outer_local_scope_depth = self.local_scope_depth;
                self.local_scope_depth += 1;
                let body_result = self.compile_block(body);
                self.local_scope_depth = outer_local_scope_depth;
                body_result?;
                self.locals = outer_locals;
                self.julia_type_locals = outer_julia_type_locals;
                self.mixed_type_vars = outer_mixed_type_vars;
                self.emit(Instr::TestSetEnd);
                Ok(())
            }
            Stmt::TestThrows {
                exception_type,
                expr,
                ..
            } => {
                // @test_throws ExceptionType expr
                // Uses try/catch pattern: if exception is thrown, it's a pass; if not, it's a fail
                let catch_start = self.here();
                self.emit(Instr::PushHandler(None, None)); // placeholder, will be patched

                // Set up test_throws state
                self.emit(Instr::TestThrowsBegin(exception_type.clone()));

                // Compile the expression that should throw
                self.compile_expr(expr)?;
                self.emit(Instr::Pop);

                // If we reach here, no exception was thrown - that's a failure
                self.emit(Instr::PopHandler);
                self.emit(Instr::TestThrowsEnd); // Will report failure (no exception)
                let jump_to_end = self.here();
                self.emit(Instr::Jump(usize::MAX)); // placeholder

                // Catch block - exception was thrown
                let catch_ip = self.here();
                self.emit(Instr::ClearError);
                self.emit(Instr::TestThrowsEnd); // Will report success

                // Patch the handler to jump to catch
                self.code[catch_start] = Instr::PushHandler(Some(catch_ip), None);

                // Patch the jump to skip catch block
                let end = self.here();
                self.code[jump_to_end] = Instr::Jump(end);

                Ok(())
            }
            Stmt::Timed { body, .. } => {
                self.emit(Instr::TimeNs);
                self.emit(Instr::StoreI64("__time_start".to_string()));

                self.compile_block(body)?;

                self.emit(Instr::TimeNs);
                self.emit(Instr::LoadI64("__time_start".to_string()));
                self.emit(Instr::SubI64);
                self.emit(Instr::ToF64);
                self.emit(Instr::PushF64(1_000_000_000.0));
                self.emit(Instr::DivF64);
                self.emit(Instr::PushStr("  ".to_string()));
                self.emit(Instr::PrintStrNoNewline);
                self.emit(Instr::PrintF64NoNewline);
                self.emit(Instr::PushStr(" seconds".to_string()));
                self.emit(Instr::PrintStr);
                Ok(())
            }
            Stmt::IndexAssign {
                array,
                indices,
                value,
                span,
            } => {
                // `d[k1, k2, ...] = v` on an AbstractDict is sugar for
                // `d[(k1, k2, ...)] = v`: upstream defines
                // `setindex!(t::AbstractDict, v, k1, k2, ks...) =
                // setindex!(t, v, tuple(k1, k2, ks...))` (abstractdict.jl). Without
                // this, a Dict target with 2+ plain indices falls through to native
                // multi-dim `IndexStore(N)`, which errors on a Dict (Issue #6707,
                // sibling of the getindex fix). Rewrite to a single tuple key and
                // dispatch the ordinary one-key setindex!.
                if indices.len() >= 2
                    && !indices
                        .iter()
                        .any(|idx| matches!(idx, Expr::Range { .. } | Expr::SliceAll { .. }))
                {
                    let target_ty = if self.declared_globals.contains(array) {
                        Some(ValueType::Any)
                    } else {
                        self.locals.get(array).cloned()
                    };
                    let target_julia = self.infer_julia_type(&Expr::Var(array.clone(), *span));
                    let target_is_dict_like = matches!(&target_ty, Some(ValueType::Dict))
                        || matches!(&target_ty, Some(ValueType::Struct(type_id))
                            if self
                                .shared_ctx
                                .type_id_to_struct_name
                                .get(type_id)
                                .is_some_and(|name| is_dict_struct_name(name)))
                        || matches!(target_julia, JuliaType::Dict)
                        || matches!(&target_julia, JuliaType::Struct(name) if is_dict_struct_name(name));
                    if target_is_dict_like {
                        let key = Expr::TupleLiteral {
                            elements: indices.clone(),
                            span: *span,
                        };
                        let new_args =
                            vec![Expr::Var(array.clone(), *span), value.clone(), key];
                        let ty = self.compile_call("setindex!", &new_args, &[], &[], &[])?;
                        if matches!(ty, ValueType::Nothing) {
                            self.emit(Instr::Pop);
                        } else {
                            let dummy = self.new_temp("discard");
                            self.emit(Instr::StoreAny(dummy));
                        }
                        return Ok(());
                    }
                }

                // Julia-compliant: arr[i] = v is equivalent to setindex!(arr, v, i)
                // We implement this directly with VM instructions for efficiency,
                // and store the modified collection back to the variable.
                let mut setindex_args = Vec::with_capacity(indices.len() + 2);
                setindex_args.push(Expr::Var(array.clone(), *span));
                setindex_args.push(value.clone());
                setindex_args.extend(indices.clone());
                let setindex_arg_types: Vec<JuliaType> = setindex_args
                    .iter()
                    .map(|arg| self.infer_julia_type(arg))
                    .collect();
                let target_ty = if self.declared_globals.contains(array) {
                    Some(ValueType::Any)
                } else {
                    self.locals.get(array).cloned()
                };
                let is_struct_backed_dict_target = match &target_ty {
                    Some(ValueType::Struct(type_id)) => self
                        .shared_ctx
                        .type_id_to_struct_name
                        .get(type_id)
                        .is_some_and(|name| is_dict_struct_name(name)),
                    _ => false,
                };
                // A DataType-valued key can only target a Dict, so route the
                // assignment through the `setindex!` builtin (which dispatches to
                // DictSet) rather than the native array-store path below. The
                // array path coerces a numeric scalar value to F64 for an
                // unboxed-target store, which would corrupt a boxed Dict value
                // (e.g. `d[T] = 1` storing `1.0`); DictSet preserves it
                // (Issue #7940).
                let has_datatype_index = indices
                    .iter()
                    .any(|idx| matches!(self.infer_expr_type(idx), ValueType::DataType));
                if is_struct_backed_dict_target
                    || has_datatype_index
                    || self.has_user_dispatch_method_for_arg_types(
                    &["setindex!", "Base.setindex!"],
                    &setindex_arg_types,
                ) {
                    let ty = self.compile_call("setindex!", &setindex_args, &[], &[], &[])?;
                    if matches!(ty, ValueType::Nothing) {
                        self.emit(Instr::Pop);
                    } else {
                        let dummy = self.new_temp("discard");
                        self.emit(Instr::StoreAny(dummy));
                    }
                    return Ok(());
                }

                // Check if this is a global variable (in global_types but not in locals)
                let is_global = self.declared_globals.contains(array)
                    || (target_ty.is_none() && self.shared_ctx.global_types.contains_key(array));
                match target_ty {
                    Some(ValueType::Dict) => {
                        // Dict assignment: setindex!(d, value, key)
                        if indices.len() != 1 {
                            return err("Dict indexing requires exactly one key");
                        }
                        self.emit(Instr::LoadDict(array.clone()));
                        self.compile_expr(&indices[0])?;
                        self.compile_expr(value)?;
                        self.emit(Instr::DictSet);
                        self.emit(Instr::StoreDict(array.clone()));
                        Ok(())
                    }
                    _ => {
                        // Array/struct assignment: setindex!(collection, value, indices...)
                        // Use typed load so StructRef (e.g., SubArray) is supported by IndexStore.
                        self.load_local(array)?;
                        for idx in indices {
                            // When collection type is Any, the index may be a non-integer
                            // key for Dict indexing. Don't force I64 conversion to allow
                            // runtime Dict dispatch (Issue #1814). (DataType keys are
                            // routed to `setindex!`/DictSet above before reaching here —
                            // Issue #7940.)
                            let idx_type = self.infer_expr_type(idx);
                            if matches!(target_ty, Some(ValueType::Any) | None)
                                && matches!(
                                    idx_type,
                                    ValueType::Any
                                        | ValueType::Tuple
                                        | ValueType::Str
                                        | ValueType::Symbol
                                )
                            {
                                self.compile_expr(idx)?;
                            } else {
                                self.compile_expr_as(idx, ValueType::I64)?;
                            }
                        }
                        // Compile value without type coercion to support tuples and other types
                        let val_ty = self.compile_expr(value)?;
                        // Only coerce to F64 if it's a numeric type (not Tuple, Struct, etc.)
                        if !target_preserves_boxed_numeric_values(target_ty.as_ref()) {
                            match val_ty {
                                ValueType::I64 | ValueType::I32 | ValueType::F32 => {
                                    self.emit(Instr::ToF64);
                                }
                                _ => {}
                            }
                        }
                        let array_expr = Expr::Var(array.clone(), *span);
                        if indices.len() == 1
                            && self.is_proven_inbounds_index(&array_expr, &indices[0])
                        {
                            self.emit(Instr::IndexStoreInbounds(indices.len()));
                        } else {
                            self.emit(Instr::IndexStore(indices.len()));
                        }
                        // For global arrays, don't emit StoreArray because:
                        // 1. Arrays are passed by reference - IndexStore modifies in place
                        // 2. StoreArray would create a local slot, shadowing the global
                        // 3. The slotized LoadSlot would then fail to find the value
                        // Instead, just pop the modified array reference from the stack.
                        if is_global {
                            self.emit(Instr::Pop);
                        } else {
                            self.emit(Instr::StoreArray(array.clone()));
                        }
                        Ok(())
                    }
                }
            }
            Stmt::FieldAssign {
                object,
                field,
                value,
                ..
            } => {
                // Get the struct type from the local variable
                let obj_ty =
                    self.locals.get(object).cloned().ok_or_else(|| {
                        CompileError::Msg(format!("Unknown variable: {}", object))
                    })?;

                match obj_ty {
                    ValueType::Struct(type_id) => {
                        // Find the struct info and field index
                        let mut field_idx = None;
                        let mut field_ty = ValueType::F64;
                        let mut is_mutable = false;
                        let mut struct_name = String::new();

                        for (name, struct_info) in self.shared_ctx.struct_table.iter() {
                            if struct_info.type_id == type_id {
                                struct_name = name.clone();
                                is_mutable = struct_info.is_mutable;
                                for (idx, (field_name, fty)) in
                                    struct_info.fields.iter().enumerate()
                                {
                                    if field_name == field {
                                        field_idx = Some(idx);
                                        field_ty = fty.clone();
                                        break;
                                    }
                                }
                                break;
                            }
                        }

                        if !is_mutable {
                            return err("Cannot assign to field of immutable struct".to_string());
                        }

                        let idx = match field_idx {
                            Some(idx) => idx,
                            None
                                if is_array_wrapper_struct_name(&struct_name)
                                    && is_array_wrapper_compat_field(field) =>
                            {
                                self.emit(Instr::LoadStruct(object.clone()));
                                self.compile_expr(value)?;
                                self.emit(Instr::SetFieldByName(field.to_string()));
                                self.emit(Instr::StoreStruct(object.clone()));
                                return Ok(());
                            }
                            None => {
                                return Err(CompileError::Msg(format!(
                                    "Unknown field: {}",
                                    field
                                )))
                            }
                        };

                        // Load the struct
                        self.emit(Instr::LoadStruct(object.clone()));

                        // Compile the new value
                        self.compile_expr_as(value, field_ty)?;

                        // Set the field
                        self.emit(Instr::SetField(idx));

                        // Store the modified struct back
                        self.emit(Instr::StoreStruct(object.clone()));

                        Ok(())
                    }
                    ValueType::Any => {
                        // The receiver type is not statically known here (e.g. a
                        // generic `where T` parameter, or a value typed `Any`). Upstream
                        // Julia resolves such field assignments at runtime: defining the
                        // method does not require the field to exist on every candidate
                        // struct, and a guarded `isdefined(G, :f) || (G.f = ...)` body is
                        // legal even when no in-scope struct declares `f`. So defer the
                        // field lookup to runtime SetFieldByName, which raises if the
                        // actual value lacks the field. Concrete-struct field validation
                        // is still enforced by the ValueType::Struct arm above, which
                        // rejects an unknown field on a statically-known struct
                        // (Issue #7941, builds on Issue #2748).
                        //
                        // Use SetFieldByName for the runtime field lookup to avoid
                        // non-deterministic compile-time struct_table iteration order.
                        self.emit(Instr::LoadAny(object.clone()));

                        // Compile the new value as Any (runtime will handle type)
                        self.compile_expr(value)?;

                        // Set the field by name at runtime (resolves correct index)
                        self.emit(Instr::SetFieldByName(field.to_string()));

                        // Store the modified struct back
                        self.emit(Instr::StoreAny(object.clone()));

                        Ok(())
                    }
                    _ => err("Field assignment requires a struct variable"),
                }
            }
            Stmt::Try { .. } => {
                err("internal: Try statement reached compile_stmt (should be handled by compile_try_stmt)")
            }
            Stmt::DestructuringAssign { targets, value, .. } => {
                // Per-position element types from a statically known tuple type, so
                // `(a, b) = f()` keeps each destructured binding type-stable rather
                // than collapsing to `Any` (Issue #5183). Computed before
                // `compile_expr` mutates compiler state.
                let elem_value_types: Vec<ValueType> = match self.infer_julia_type(value) {
                    JuliaType::TupleOf(elems) => elems
                        .iter()
                        .map(|jt| self.julia_type_to_value_type_resolved(jt))
                        .collect(),
                    _ => Vec::new(),
                };

                // Compile the tuple value
                let ty = self.compile_expr(value)?;
                if ty != ValueType::Tuple {
                    return err("Destructuring assignment requires a tuple");
                }
                // Store the tuple temporarily
                let temp_tuple = self.new_temp("tuple");
                self.emit(Instr::StoreTuple(temp_tuple.clone()));

                // Extract each element and assign to targets
                for (i, target) in targets.iter().enumerate() {
                    self.emit(Instr::LoadTuple(temp_tuple.clone()));
                    self.emit(Instr::PushI64((i + 1) as i64));
                    self.emit(Instr::TupleGet);
                    // Sharpen the slot to the static element type for the numeric
                    // cases that have a sound coercion from the dynamic `TupleGet`
                    // result; everything else stays on the dynamic `Any` path so
                    // load/store representations remain consistent (Issue #5183).
                    match elem_value_types.get(i) {
                        Some(ValueType::I64) => {
                            self.emit(Instr::DynamicToI64);
                            self.emit(Instr::StoreI64(target.clone()));
                            self.locals.insert(target.clone(), ValueType::I64);
                        }
                        Some(ValueType::F64) => {
                            self.emit(Instr::DynamicToF64);
                            self.emit(Instr::StoreF64(target.clone()));
                            self.locals.insert(target.clone(), ValueType::F64);
                        }
                        _ => {
                            // Tuple element type is unknown / not coercible — use Any
                            self.emit(Instr::StoreAny(target.clone()));
                            self.locals.insert(target.clone(), ValueType::Any);
                        }
                    }
                }
                Ok(())
            }
            Stmt::DictAssign {
                dict, key, value, ..
            } => {
                // dict[key] = value
                self.emit(Instr::LoadDict(dict.clone()));
                self.compile_expr(key)?;
                self.compile_expr(value)?;
                self.emit(Instr::DictSet);
                self.emit(Instr::StoreDict(dict.clone()));
                Ok(())
            }
            Stmt::Using { .. } => {
                // Using statements are processed at the program level,
                // not during statement compilation. They're already
                // collected in program.usings for function resolution.
                Ok(())
            }
            Stmt::Export { .. } => {
                // Export statements are processed at the module level,
                // not during statement compilation. They're already
                // collected in module.exports.
                Ok(())
            }
            Stmt::FunctionDef { func, .. } => {
                // Function definitions inside blocks (e.g., inside @testset, or nested functions).
                // The function has already been compiled during the initial compilation pass.

                // Create a qualified function name for disambiguation when multiple parent
                // functions have nested functions with the same name (Issue #1743).
                // Format: "parent_function#nested_function"
                let qualified_name = if let Some(parent_name) = &self.current_function_name {
                    format!("{}#{}", parent_name, func.name)
                } else {
                    func.name.clone()
                };

                // Check if this is a nested function that needs to capture variables
                // from the enclosing scope (closure).
                // This runs at BOTH function level (strict_undefined_check=true) AND
                // module level (strict_undefined_check=false) to support closures defined
                // at top-level or in @testset blocks (Issue #2358).
                // Include both local variables AND captured variables from ancestor scopes
                // to support 3+ levels of closure nesting (Issue #1744)
                // Issue #8118: a nested function in a mutually-recursive closure
                // group that captures an enclosing local uses the authoritative
                // capture set computed up-front by
                // `prescan_mutual_closure_captures` (enclosing-scope data only,
                // sibling function names excluded, sibling captures propagated in).
                // Recomputing free variables here would re-capture sibling names
                // and miss the transitive propagation, breaking reconstruction.
                let free_vars = if let Some(prescanned) =
                    self.mutual_closure_captures.get(&qualified_name).cloned()
                {
                    prescanned
                } else {
                    let mut outer_scope_vars: HashSet<String> =
                        self.locals.keys().cloned().collect();
                    outer_scope_vars.extend(self.captured_vars.iter().cloned());
                    analyze_free_variables(func, &outer_scope_vars)
                };

                if !free_vars.is_empty() {
                    // This is a closure - store capture info for when the function is compiled
                    // Use qualified name to avoid collision between nested functions with same name
                    self.shared_ctx
                        .closure_captures
                        .insert(qualified_name.clone(), free_vars.clone());

                    // Emit CreateClosure with the QUALIFIED function name
                    // FunctionInfo.name also uses the qualified name for nested functions,
                    // so the runtime lookup will find the correct function (Issue #1743)
                    let capture_names: Vec<String> = free_vars.into_iter().collect();
                    self.emit(Instr::CreateClosure {
                        func_name: qualified_name,
                        capture_names,
                    });
                    // Store the closure in the local scope using the ORIGINAL name
                    // (so the local variable `inner` can be accessed normally in user code)
                    self.emit(Instr::StoreAny(func.name.clone()));
                    self.locals.insert(func.name.clone(), ValueType::Any);
                    return Ok(());
                }

                // Regular function definition (not a closure)
                // For nested functions (inside other functions), we need to make them
                // accessible as local variables so they can be called by name.
                if self.strict_undefined_check {
                    // Inside a function body - store as a Function value in local scope
                    self.emit_function_value(&qualified_name);
                    self.emit(Instr::StoreAny(func.name.clone()));
                    self.locals.insert(func.name.clone(), ValueType::Function);
                }

                // Look it up by qualified name and emit a DefineFunction instruction.
                // This instruction is a no-op at runtime but marks that the function
                // definition was executed.
                if let Some(idx) = self.shared_ctx.function_indices.get(&qualified_name) {
                    self.emit(Instr::DefineFunction(*idx));
                }
                // Even if not found, this is OK - the function might be defined
                // elsewhere or be a forward reference.
                Ok(())
            }
            Stmt::EvalFunctionDef { func, .. } => {
                if let Some(idx) = self.shared_ctx.function_indices.get(&func.name) {
                    self.emit(Instr::DefineEvalFunction(*idx));
                }
                Ok(())
            }
            Stmt::Label { name, .. } => {
                // Record the label position for @goto to jump to.
                // The label marks the current instruction position.
                let position = self.here();
                self.label_positions.insert(name.clone(), position);
                Ok(())
            }
            Stmt::Goto { name, span } => {
                // Emit a Jump instruction and record it for patching.
                // We use usize::MAX as a placeholder, which will be patched
                // after all labels are collected.
                let patch_position = self.here();
                self.emit(Instr::Jump(usize::MAX));
                self.goto_patches.push((patch_position, name.clone()));
                // Note: The patch will be applied after compilation by patch_goto_jumps()
                let _ = span; // Span is kept for potential future error reporting
                Ok(())
            }
            Stmt::EnumDef { enum_def, .. } => {
                // @enum runtime integration (Issue #5139).
                //
                // 1. Register the type + members in the thread-local runtime enum
                //    registry so display, `Color(v)` construction, and
                //    `instances(Color)` can recover member names / order.
                // 2. Bind each member name to its `Value::Enum` global, so bare
                //    references (`red`) resolve at runtime instead of raising
                //    UndefVarError.
                let type_name = enum_def.name.clone();
                let members: Vec<(String, i64)> = enum_def
                    .members
                    .iter()
                    .map(|m| (m.name.clone(), m.value))
                    .collect();

                self.emit(Instr::RegisterEnum(Box::new(
                    crate::vm::instr::RegisterEnumOperands {
                        type_name: type_name.clone(),
                        members: members.clone(),
                    },
                )));

                for (member_name, value) in &members {
                    // Mark the member as an Enum type in global_types so loads
                    // and stores use the dynamic (LoadAny/StoreAny) path.
                    self.shared_ctx
                        .global_types
                        .insert(member_name.clone(), ValueType::Enum);
                    self.emit(Instr::PushEnum {
                        type_name: type_name.clone(),
                        value: *value,
                    });
                    self.emit(Instr::StoreAny(member_name.clone()));
                }
                Ok(())
            }
        }
    }

    // ==========================================================================
    // Iteration Protocol Helpers
    // ==========================================================================

    /// Issue #5168: lower `for var in coll` for the builtin (non pure-Julia)
    /// iterate path without allocating a `(element, state)` tuple per iteration.
    ///
    /// `IterateFirstSplit` / `IterateNextSplit` push `[state, element]` plus a
    /// `Bool(true)` flag when a value is produced, or just `Bool(false)` when the
    /// collection is exhausted. `JumpIfZero` consumes the flag: on exhaustion it
    /// branches to the loop exit (stack already empty); otherwise `[state, element]`
    /// is left on the stack and the element / state are stored directly. This
    /// avoids the per-iteration tuple heap allocation plus the `TupleFirst` /
    /// `TupleSecond` clones of the prior lowering.
    fn compile_foreach_split(&mut self, var: &str, iterable: &Expr, body: &Block) -> CResult<()> {
        let iterable_var = self.new_temp("iterable");
        let state_var = self.new_temp("state");

        // Store the iterable.
        self.compile_expr(iterable)?;
        self.emit(Instr::StoreAny(iterable_var.clone()));

        // First iteration: iterate(collection).
        self.emit(Instr::LoadAny(iterable_var.clone()));
        self.emit(Instr::IterateFirstSplit);
        // Stack: [state, element, Bool(true)] or [Bool(false)].
        let j_to_exit_first = self.here();
        self.emit(Instr::JumpIfZero(usize::MAX)); // Exit if exhausted (flag false).

        // Value present: stack is [state, element]; element on top.
        self.emit(Instr::StoreAny(var.to_string())); // pop element -> loop var
        self.emit(Instr::StoreAny(state_var.clone())); // pop state -> state slot
        self.locals.insert(var.to_string(), ValueType::Any);

        let loop_start = self.here();

        // Push loop context for break/continue.
        let loop_ctx = LoopContext {
            exit_patches: vec![j_to_exit_first],
            continue_patches: Vec::new(),
        };
        let inbounds_array_var = proven_inbounds_loop_array_var(iterable);
        if let Some(array_var) = inbounds_array_var {
            self.push_proven_inbounds_index(array_var, var);
        }
        self.loop_stack.push(loop_ctx);
        let body_result = self.compile_block(body);
        let loop_ctx = self.loop_stack.pop().unwrap();
        if inbounds_array_var.is_some() {
            self.pop_proven_inbounds_index();
        }
        body_result?;

        let continue_target = self.here();

        // Next iteration: iterate(collection, state).
        self.emit(Instr::LoadAny(iterable_var.clone()));
        self.emit(Instr::LoadAny(state_var.clone()));
        self.emit(Instr::IterateNextSplit);
        // Stack: [state, element, Bool(true)] or [Bool(false)].
        let j_to_exit_loop = self.here();
        self.emit(Instr::JumpIfZero(usize::MAX)); // Exit if exhausted (flag false).

        // Value present: stack is [state, element]; element on top.
        self.emit(Instr::StoreAny(var.to_string())); // pop element -> loop var
        self.emit(Instr::StoreAny(state_var.clone())); // pop state -> state slot
        self.emit(Instr::Jump(loop_start));

        let exit = self.here();

        // Patch exit jumps (first/next exhaustion + any break statements).
        self.patch_jump(j_to_exit_first, exit);
        self.patch_jump(j_to_exit_loop, exit);
        for patch_pos in loop_ctx.exit_patches {
            if patch_pos != j_to_exit_first {
                self.patch_jump(patch_pos, exit);
            }
        }
        for patch_pos in loop_ctx.continue_patches {
            self.patch_jump(patch_pos, continue_target);
        }

        Ok(())
    }

    /// Check if we should use Pure Julia iterate for this type.
    /// Returns true for struct types (custom iterators), false for builtin types.
    pub(in crate::compile) fn should_use_pure_julia_iterate(&self, ty: &JuliaType) -> bool {
        if let Some(result) = static_iterate_strategy(ty) {
            return result;
        }
        // Dynamic fallback: check if there's an iterate method registered
        if let Some(table) = self.method_tables.get("iterate") {
            !table.methods.is_empty()
        } else {
            false
        }
    }

    /// Emit a call to iterate(collection) - 1 argument version.
    /// Looks up the iterate method from method tables and emits a Call instruction.
    pub(in crate::compile) fn emit_iterate_call_1(&mut self, ty: &JuliaType) -> CResult<()> {
        if let Some(table) = self.method_tables.get("iterate") {
            let arg_types = vec![ty.clone()];
            if let Ok(method) = table.dispatch(&arg_types) {
                self.emit(Instr::Call(method.global_index, 1));
                return Ok(());
            }
            // Try Any dispatch
            let arg_types_any = vec![JuliaType::Any];
            if let Ok(method) = table.dispatch(&arg_types_any) {
                self.emit(Instr::Call(method.global_index, 1));
                return Ok(());
            }
            // For Any type, use IterateDynamic for runtime struct dispatch
            // This handles cases where the collection is a struct type unknown at compile time
            // (e.g., zip(a, b, c) returns Any, but at runtime it's Zip3)
            if matches!(ty, JuliaType::Any) {
                let candidates: Vec<usize> = table
                    .methods
                    .iter()
                    .filter(|m| m.param_count() == 1)
                    .filter_map(|m| {
                        let ty = m.projected_param_julia_type(0);
                        Self::is_stmt_runtime_iterate_candidate_type(ty.as_ref())
                            .then_some(m.global_index)
                    })
                    .collect();
                if !candidates.is_empty() {
                    self.emit(Instr::IterateDynamic(1, candidates));
                    return Ok(());
                }
            }
        }
        // Fall back to VM instruction - handles Array, Tuple, String, Range at runtime
        self.emit(Instr::IterateFirst);
        Ok(())
    }

    /// Emit a call to iterate(collection, state) - 2 argument version.
    /// Looks up the iterate method from method tables and emits a Call instruction.
    pub(in crate::compile) fn emit_iterate_call_2(&mut self, ty: &JuliaType) -> CResult<()> {
        if let Some(table) = self.method_tables.get("iterate") {
            // Try to find method with (collection_type, Int64) signature
            let arg_types = vec![ty.clone(), JuliaType::Int64];
            if let Ok(method) = table.dispatch(&arg_types) {
                self.emit(Instr::Call(method.global_index, 2));
                return Ok(());
            }
            // Try with Any as second argument
            let arg_types_any = vec![ty.clone(), JuliaType::Any];
            if let Ok(method) = table.dispatch(&arg_types_any) {
                self.emit(Instr::Call(method.global_index, 2));
                return Ok(());
            }
            // Try with both as Any
            let arg_types_both_any = vec![JuliaType::Any, JuliaType::Any];
            if let Ok(method) = table.dispatch(&arg_types_both_any) {
                self.emit(Instr::Call(method.global_index, 2));
                return Ok(());
            }
            // For Any type, use IterateDynamic for runtime struct dispatch
            if matches!(ty, JuliaType::Any) {
                let candidates: Vec<usize> = table
                    .methods
                    .iter()
                    .filter(|m| m.param_count() == 2)
                    .filter_map(|m| {
                        let ty = m.projected_param_julia_type(0);
                        Self::is_stmt_runtime_iterate_candidate_type(ty.as_ref())
                            .then_some(m.global_index)
                    })
                    .collect();
                if !candidates.is_empty() {
                    self.emit(Instr::IterateDynamic(2, candidates));
                    return Ok(());
                }
            }
        }
        // Fall back to VM instruction - handles Array, Tuple, String, Range at runtime
        self.emit(Instr::IterateNext);
        Ok(())
    }

    fn is_stmt_runtime_iterate_candidate_type(julia_type: &JuliaType) -> bool {
        matches!(
            julia_type,
            JuliaType::Struct(_)
                | JuliaType::Array
                | JuliaType::VectorOf(_)
                | JuliaType::MatrixOf(_)
                // `Set` is a pure-Julia struct over `Dict{T,Nothing}` (Issue
                // #6721); a bare `::Set` (or `::Dict`) iterate method annotation
                // resolves to the native carrier `JuliaType`, but the value is a
                // `StructRef`, so include it as a runtime IterateDynamic candidate
                // (e.g. `for x in itr` where `itr::Any` binds a Set struct inside
                // `union!`).
                | JuliaType::Set
                | JuliaType::Dict
        )
    }
}

/// Collect names declared `global` anywhere in a single local scope (`block`),
/// recursing into nested control-flow blocks but NOT into nested function
/// definitions, which introduce their own scope. See `compile_function_body`
/// (Issues #5548, #5549).
fn collect_declared_globals(block: &Block, out: &mut HashSet<String>) {
    for stmt in &block.stmts {
        collect_declared_globals_in_stmt(stmt, out);
    }
}

fn collect_declared_globals_in_stmt(stmt: &Stmt, out: &mut HashSet<String>) {
    match stmt {
        Stmt::Global { names, .. } => {
            for name in names {
                out.insert(name.clone());
            }
        }
        Stmt::Block(block) => collect_declared_globals(block, out),
        Stmt::If {
            then_branch,
            else_branch,
            ..
        } => {
            collect_declared_globals(then_branch, out);
            if let Some(block) = else_branch {
                collect_declared_globals(block, out);
            }
        }
        Stmt::For { body, .. }
        | Stmt::ForEach { body, .. }
        | Stmt::ForEachTuple { body, .. }
        | Stmt::While { body, .. }
        | Stmt::Timed { body, .. }
        | Stmt::TestSet { body, .. } => collect_declared_globals(body, out),
        Stmt::Try {
            try_block,
            catch_block,
            else_block,
            finally_block,
            ..
        } => {
            collect_declared_globals(try_block, out);
            if let Some(block) = catch_block {
                collect_declared_globals(block, out);
            }
            if let Some(block) = else_block {
                collect_declared_globals(block, out);
            }
            if let Some(block) = finally_block {
                collect_declared_globals(block, out);
            }
        }
        // Other statements never introduce `global` declarations for this scope.
        // `Stmt::FunctionDef` is intentionally skipped: a nested function is a
        // new local scope with its own declarations.
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::JuliaType;
    use crate::vm::ValueType;

    // ── static_iterate_strategy ───────────────────────────────────────────────

    #[test]
    fn test_static_iterate_strategy_struct_uses_pure_julia() {
        let ty = JuliaType::Struct("Point".to_string());
        assert_eq!(static_iterate_strategy(&ty), Some(true));
    }

    #[test]
    fn test_static_iterate_strategy_cartesian_indices_uses_builtin() {
        let ty = JuliaType::Struct("CartesianIndices".to_string());
        assert_eq!(
            static_iterate_strategy(&ty),
            Some(false),
            "CartesianIndices is special-cased to use VM builtin iterate"
        );
    }

    #[test]
    fn test_static_iterate_strategy_any_uses_pure_julia() {
        assert_eq!(
            static_iterate_strategy(&JuliaType::Any),
            Some(true),
            "Any uses Pure Julia dispatch for runtime struct resolution"
        );
    }

    #[test]
    fn test_static_iterate_strategy_array_types_use_builtin() {
        assert_eq!(static_iterate_strategy(&JuliaType::Array), Some(false));
        assert_eq!(
            static_iterate_strategy(&JuliaType::VectorOf(Box::new(JuliaType::Int64))),
            Some(false)
        );
        assert_eq!(
            static_iterate_strategy(&JuliaType::MatrixOf(Box::new(JuliaType::Float64))),
            Some(false)
        );
    }

    #[test]
    fn test_static_iterate_strategy_tuple_types_use_builtin() {
        assert_eq!(static_iterate_strategy(&JuliaType::Tuple), Some(false));
        assert_eq!(
            static_iterate_strategy(&JuliaType::TupleOf(vec![JuliaType::Int64])),
            Some(false)
        );
    }

    #[test]
    fn test_static_iterate_strategy_string_uses_builtin() {
        assert_eq!(static_iterate_strategy(&JuliaType::String), Some(false));
    }

    #[test]
    fn test_static_iterate_strategy_int64_uses_builtin() {
        // Range-like types use VM builtin iterate
        assert_eq!(static_iterate_strategy(&JuliaType::Int64), Some(false));
    }

    #[test]
    fn test_static_iterate_strategy_unknown_types_return_none() {
        // These types require runtime method-table lookup
        assert_eq!(static_iterate_strategy(&JuliaType::Bool), None);
        assert_eq!(static_iterate_strategy(&JuliaType::Float64), None);
        assert_eq!(static_iterate_strategy(&JuliaType::Dict), None);
    }

    // ── const_bool_condition (Issue #5182) ────────────────────────────────────

    fn sp() -> crate::span::Span {
        crate::span::Span::new(0, 0, 0, 0, 0, 0)
    }

    fn lit_int(v: i64) -> Expr {
        Expr::Literal(Literal::Int(v), sp())
    }

    fn lit_bool(v: bool) -> Expr {
        Expr::Literal(Literal::Bool(v), sp())
    }

    fn binop(op: crate::ir::core::BinaryOp, left: Expr, right: Expr) -> Expr {
        Expr::BinaryOp {
            op,
            left: Box::new(left),
            right: Box::new(right),
            span: sp(),
        }
    }

    #[test]
    fn test_const_bool_condition_bare_bool_literal() {
        // The trivial Issue #3364 case must still fold.
        assert_eq!(const_bool_condition(&lit_bool(true)), Some(true));
        assert_eq!(const_bool_condition(&lit_bool(false)), Some(false));
    }

    #[test]
    fn test_const_bool_condition_comparisons_are_not_folded() {
        // Comparison/equality operators dispatch to user-overridable methods
        // (Issue #4298), so they must NOT be folded for dead-branch elimination
        // even on constant operands — otherwise `if "a" == "a"` with a user
        // `==(::String,::String)=false` would be eliminated to the wrong branch.
        use crate::ir::core::BinaryOp;
        assert_eq!(
            const_bool_condition(&binop(BinaryOp::Lt, lit_int(1), lit_int(2))),
            None
        );
        assert_eq!(
            const_bool_condition(&binop(BinaryOp::Gt, lit_int(1), lit_int(2))),
            None
        );
        assert_eq!(
            const_bool_condition(&binop(BinaryOp::Eq, lit_int(3), lit_int(3))),
            None
        );
    }

    #[test]
    fn test_const_bool_condition_boolean_algebra() {
        // `true && false` -> false, `false || true` -> true.
        use crate::ir::core::BinaryOp;
        assert_eq!(
            const_bool_condition(&binop(BinaryOp::And, lit_bool(true), lit_bool(false))),
            Some(false)
        );
        assert_eq!(
            const_bool_condition(&binop(BinaryOp::Or, lit_bool(false), lit_bool(true))),
            Some(true)
        );
    }

    #[test]
    fn test_const_bool_condition_unary_not() {
        // `!false` -> true (dispatch-free). `!(1 == 2)` wraps a comparison, which
        // is NOT dispatch-free (Issue #4298), so it must NOT fold -> None.
        use crate::ir::core::{BinaryOp, UnaryOp};
        let not_false = Expr::UnaryOp {
            op: UnaryOp::Not,
            operand: Box::new(lit_bool(false)),
            span: sp(),
        };
        assert_eq!(const_bool_condition(&not_false), Some(true));

        let not_eq = Expr::UnaryOp {
            op: UnaryOp::Not,
            operand: Box::new(binop(BinaryOp::Eq, lit_int(1), lit_int(2))),
            span: sp(),
        };
        assert_eq!(const_bool_condition(&not_eq), None);
    }

    #[test]
    fn test_const_bool_condition_nested_expression() {
        // `(1 + 1) < 3 && 2 * 2 == 4` contains comparison operators, which are
        // dispatch-bearing (Issue #4298) — the whole condition must NOT fold even
        // though its operands are constant. Returns None (no dead-branch elim).
        use crate::ir::core::BinaryOp;
        let lhs = binop(
            BinaryOp::Lt,
            binop(BinaryOp::Add, lit_int(1), lit_int(1)),
            lit_int(3),
        );
        let rhs = binop(
            BinaryOp::Eq,
            binop(BinaryOp::Mul, lit_int(2), lit_int(2)),
            lit_int(4),
        );
        assert_eq!(const_bool_condition(&binop(BinaryOp::And, lhs, rhs)), None);
    }

    #[test]
    fn test_const_bool_condition_non_bool_result_is_none() {
        // A const expression that folds to an Int (not Bool) is not a usable
        // condition for branch elimination — must return None.
        use crate::ir::core::BinaryOp;
        assert_eq!(
            const_bool_condition(&binop(BinaryOp::Add, lit_int(1), lit_int(2))),
            None
        );
    }

    #[test]
    fn test_const_bool_condition_variable_is_none() {
        // A runtime variable cannot be folded — DCE must not fire.
        let var = Expr::Var("x".to_string(), sp());
        assert_eq!(const_bool_condition(&var), None);
    }

    #[test]
    fn test_const_bool_condition_call_is_none() {
        // An impure / unknown call must never fold (side effects, runtime value).
        let call = Expr::Call {
            function: "f".to_string(),
            args: vec![],
            kwargs: vec![],
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span: sp(),
        };
        assert_eq!(const_bool_condition(&call), None);
    }

    // ── can_convert_type ──────────────────────────────────────────────────────

    #[test]
    fn test_can_convert_i64_to_f64() {
        assert!(
            can_convert_type(ValueType::I64, ValueType::F64),
            "I64 → F64 conversion should be supported"
        );
    }

    #[test]
    fn test_can_convert_f64_to_i64() {
        assert!(
            can_convert_type(ValueType::F64, ValueType::I64),
            "F64 → I64 conversion should be supported"
        );
    }

    #[test]
    fn test_cannot_convert_same_type() {
        assert!(
            !can_convert_type(ValueType::I64, ValueType::I64),
            "I64 → I64 is not a conversion (same type)"
        );
        assert!(
            !can_convert_type(ValueType::F64, ValueType::F64),
            "F64 → F64 is not a conversion (same type)"
        );
    }

    #[test]
    fn test_cannot_convert_unrelated_types() {
        assert!(
            !can_convert_type(ValueType::Bool, ValueType::I64),
            "Bool → I64 is not a direct VM conversion"
        );
        assert!(
            !can_convert_type(ValueType::Str, ValueType::Any),
            "Str → Any is not a direct VM conversion"
        );
        assert!(
            !can_convert_type(ValueType::I64, ValueType::Bool),
            "I64 → Bool is not a direct VM conversion"
        );
        assert!(
            !can_convert_type(ValueType::F32, ValueType::F64),
            "F32 → F64 is not a direct VM conversion (no dedicated instruction)"
        );
    }

    #[test]
    fn test_cannot_convert_any_to_concrete() {
        assert!(
            !can_convert_type(ValueType::Any, ValueType::I64),
            "Any → I64 is not a direct VM conversion"
        );
        assert!(
            !can_convert_type(ValueType::Any, ValueType::F64),
            "Any → F64 is not a direct VM conversion"
        );
    }

    #[test]
    fn test_any_return_can_use_declared_primitive_return_opcode() {
        assert!(should_return_as_expected_type(
            &ValueType::Any,
            &ValueType::I64
        ));
        assert!(should_return_as_expected_type(
            &ValueType::Any,
            &ValueType::F64
        ));
        assert!(!should_return_as_expected_type(
            &ValueType::Any,
            &ValueType::Str
        ));
    }
}
