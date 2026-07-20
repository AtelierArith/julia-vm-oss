//! Expression compilation for CoreCompiler.
//!
//! This module contains expression-level compilation methods including
//! literal handling, binary/unary operations, function calls, and builtins.
//!
//! Submodules:
//! - `binary`: Binary operation compilation
//! - `builtin`: Builtin function compilation
//! - `call`: Function call compilation
//! - `collection`: Collection (array, dict) compilation
//! - `infer`: Type inference
//! - `struct_`: Struct compilation
//! - `unary`: Unary operation compilation

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

// `pub(crate)` so the Base-corpus parity gates in `compile::cache::tests` can
// referee the CoreType-native binary dispatch heuristics (Issue #6495, 6b-ii).
pub(crate) mod binary;
mod builtin;
mod builtin_array;
mod builtin_hof;
mod builtin_io;
mod builtin_math;
// builtin_set removed (Issue #3724): Set algebra now Pure Julia (base/set.jl)
mod builtin_string;
mod builtin_types;
// `pub(crate)` so the Base-corpus parity gates in `compile::cache::tests` can
// referee the CoreType-native call dispatch heuristics (Issue #6495, 6b-ii).
pub(crate) mod call;
mod coercion;
mod collection;
mod infer;
mod struct_;
mod unary;

pub(crate) use infer::{infer_array_element_type, infer_nested_array_literal_element_type};

use crate::builtins::BuiltinId;
use crate::bytecode::{ArrayElementType, ArrayLiteralPayload, Instr, ModuleOperands, ValueType};
use crate::ir::core::{Block, BuiltinOp, Expr, Literal, NumericConvertTarget, Stmt};
use crate::types::{JuliaType, TypeExpr};
use half::f16;
use std::collections::HashSet;

use super::core_compiler::ScopeCleanupContext;
use super::types::{err, CResult, CompileError};
use super::{
    get_math_constant_value, is_base_function, is_builtin_type_name, is_euler_name, is_pi_name,
    is_random_function, parse_parametric_call, CoreCompiler,
};

fn is_julia_array_like_type(ty: &JuliaType) -> bool {
    matches!(
        ty,
        JuliaType::Array | JuliaType::VectorOf(_) | JuliaType::MatrixOf(_)
    ) || matches!(ty, JuliaType::Struct(name)
        if name == "Array"
            || name.starts_with("Array{")
            || name.starts_with("Vector{")
            || name.starts_with("Matrix{"))
}

fn is_range_like_julia_type(ty: &JuliaType) -> bool {
    match ty {
        JuliaType::UnitRange | JuliaType::StepRange | JuliaType::AbstractRange => true,
        JuliaType::Struct(name) => {
            let base = name
                .split('{')
                .next()
                .unwrap_or(name.as_str())
                .rsplit('.')
                .next()
                .unwrap_or(name.as_str());
            matches!(
                base,
                "UnitRange" | "StepRange" | "StepRangeLen" | "LinRange" | "OneTo" | "LogRange"
            )
        }
        _ => false,
    }
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

fn concrete_range_constructor_name(ty: &JuliaType) -> Option<&'static str> {
    let JuliaType::Struct(name) = ty else {
        return None;
    };
    let base = name.split('{').next().unwrap_or(name);
    let base = base.rsplit('.').next().unwrap_or(base);
    match base {
        "UnitRange" => Some("UnitRange"),
        "StepRange" => Some("StepRange"),
        // Keep float colon ranges on the existing native StepRangeLen path so
        // TwicePrecision-compatible length/index semantics stay intact.
        _ => None,
    }
}

impl<'a> CoreCompiler<'a> {
    fn emit_builtin_irrational_singleton(&mut self, name: &str) -> Option<ValueType> {
        let symbol = if is_pi_name(name) {
            "π"
        } else if is_euler_name(name) {
            "ℯ"
        } else {
            return None;
        };
        let type_name = format!("Irrational{{:{}}}", symbol);
        let type_id = if let Some(info) = self.shared_ctx.struct_table.get(&type_name) {
            info.type_id
        } else {
            let type_arg = TypeExpr::RuntimeExpr(format!(":{}", symbol));
            self.shared_ctx
                .resolve_instantiation_with_type_expr("Irrational", &[type_arg])
                .ok()?
        };
        self.emit(Instr::NewStruct(type_id, 0));
        Some(ValueType::Struct(type_id))
    }

    fn module_private_type_object_name(&self, name: &str) -> Option<String> {
        if self.locals.contains_key(name) || name.contains('.') {
            return None;
        }
        let module_path = self.current_module_path.as_ref()?;
        let qualified = format!("{}.{}", module_path, name);
        (self.shared_ctx.struct_table.contains_key(&qualified)
            || self.shared_ctx.parametric_structs.contains_key(&qualified)
            || self.abstract_type_names.contains(&qualified)
            || self.shared_ctx.enum_types.contains_key(&qualified)
            || self.shared_ctx.is_primitive_type_name(&qualified))
        .then_some(qualified)
    }
}

fn block_opens_testset_scope(block: &Block) -> bool {
    block.stmts.iter().any(|stmt| match stmt {
        Stmt::Expr { expr, .. } => expr_opens_testset_scope(expr),
        _ => false,
    })
}

fn expr_opens_testset_scope(expr: &Expr) -> bool {
    match expr {
        Expr::Builtin {
            name: BuiltinOp::TestSetBegin,
            ..
        } => true,
        Expr::Call { function, .. } => function == "_testset_begin!",
        Expr::LetBlock { body, .. } => block_opens_testset_scope(body),
        _ => false,
    }
}

/// True when a builtin [`JuliaType`] names a type that declares NO type
/// parameters — never a `UnionAll` base — so a literal type application
/// `Base{...}` on it must raise the upstream-shaped `TypeError`
/// (`jl_apply_type` requires a `UnionAll` base), exactly like
/// `Core.apply_type(Base, ...)` does since #10554/#10587 (Issue #10654).
///
/// Enumerated over `JuliaType` variants, not name strings: parametric builtin
/// families (`Array`, `Dict`, `Tuple`, `Type`, ranges, ...) and nominal
/// `Struct`/user names keep the permissive static fast path, so a family this
/// list does not know about can never be rejected falsely.
fn builtin_type_is_non_parametric(ty: &JuliaType) -> bool {
    matches!(
        ty,
        JuliaType::Int8
            | JuliaType::Int16
            | JuliaType::Int32
            | JuliaType::Int64
            | JuliaType::Int128
            | JuliaType::BigInt
            | JuliaType::UInt8
            | JuliaType::UInt16
            | JuliaType::UInt32
            | JuliaType::UInt64
            | JuliaType::UInt128
            | JuliaType::Bool
            | JuliaType::Float16
            | JuliaType::Float32
            | JuliaType::Float64
            | JuliaType::BigFloat
            | JuliaType::String
            | JuliaType::Char
            | JuliaType::Nothing
            | JuliaType::Missing
            | JuliaType::Any
            | JuliaType::Number
            | JuliaType::Real
            | JuliaType::Integer
            | JuliaType::Signed
            | JuliaType::Unsigned
            | JuliaType::AbstractFloat
            | JuliaType::AbstractString
            | JuliaType::AbstractChar
            | JuliaType::Symbol
            | JuliaType::Module
            | JuliaType::DataType
            | JuliaType::Function
            | JuliaType::IO
            | JuliaType::IOBuffer
            | JuliaType::Expr
            | JuliaType::QuoteNode
            | JuliaType::LineNumberNode
            | JuliaType::GlobalRef
            | JuliaType::Bottom
    )
}

/// Split the top-level `{...}` argument list of an applied type name literal
/// (`"Dict{String, Int64}"` -> `["String", "Int64"]`), respecting nested
/// braces/parens/brackets. Returns `None` unless the name is exactly one
/// `Base{args}` application of a bare base (Issue #10654).
fn split_literal_type_application(name: &str) -> Option<(&str, Vec<String>)> {
    let open = name.find('{')?;
    if open == 0 || !name.ends_with('}') {
        return None;
    }
    let base = &name[..open];
    let inner = &name[open + 1..name.len() - 1];
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, ch) in inner.char_indices() {
        match ch {
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => {
                // An unmatched close means `name` was not a single top-level
                // application (e.g. `A{B}.C{D}` shapes); decline.
                depth = depth.checked_sub(1)?;
            }
            ',' if depth == 0 => {
                args.push(inner[start..i].trim().to_string());
                start = i + ch.len_utf8();
            }
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }
    let last = inner[start..].trim();
    if !last.is_empty() {
        args.push(last.to_string());
    }
    Some((base, args))
}

/// For a literal applied type name `Base{...}` whose base is a builtin type
/// that declares no type parameters (`Int64{Float64}`, `Real{Int64}`,
/// `Any{Int64}`...), return the base and the top-level argument list so the
/// compiler can route the application through the runtime `ApplyTypeDynamic`
/// validator — the SAME path `Core.apply_type` uses — which raises the
/// upstream `TypeError: in Type{...} expression, expected UnionAll, ...`
/// instead of silently fabricating a nonsense `DataType` (Issue #10654).
fn literal_non_parametric_type_application(name: &str) -> Option<(&str, Vec<String>)> {
    // Fast path: bare names (no braces) are by far the common case.
    if !name.contains('{') {
        return None;
    }
    let (base, args) = split_literal_type_application(name)?;
    // Only bare identifier bases: qualified (`Base.Int64`) and expression
    // bases resolve through their own machinery.
    if !base
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '!')
    {
        return None;
    }
    let base_ty = JuliaType::from_name(base)?;
    if builtin_type_is_non_parametric(&base_ty) {
        Some((base, args))
    } else {
        None
    }
}

fn literal_typeof_datatype_name(lit: &Literal) -> Option<String> {
    match lit {
        Literal::Int(_) => Some("Int64".to_string()),
        Literal::Int128(_) => Some("Int128".to_string()),
        Literal::BigInt(_) => Some("BigInt".to_string()),
        Literal::BigFloat(_) => Some("BigFloat".to_string()),
        Literal::Bool(_) => Some("Bool".to_string()),
        Literal::Float(_) => Some("Float64".to_string()),
        Literal::Float32(_) => Some("Float32".to_string()),
        Literal::Float16(_) => Some("Float16".to_string()),
        Literal::Char(_) | Literal::CharMalformed(_) => Some("Char".to_string()),
        Literal::Nothing => Some("Nothing".to_string()),
        Literal::Missing => Some("Missing".to_string()),
        Literal::Symbol(_) => Some("Symbol".to_string()),
        Literal::DataType(type_name) => match JuliaType::from_name(type_name) {
            Some(JuliaType::Union(_)) => Some("Union".to_string()),
            Some(JuliaType::Bottom) => Some("Core.TypeofBottom".to_string()),
            _ => Some("DataType".to_string()),
        },
        Literal::Module(_) => Some("Module".to_string()),
        Literal::Regex { .. } => Some("Regex".to_string()),
        Literal::Enum { .. } => Some("Enum".to_string()),
        Literal::Str(_)
        | Literal::StrBytes(_)
        | Literal::Array(_, _)
        | Literal::ArrayI64(_, _)
        | Literal::ArrayBool(_, _)
        | Literal::Struct(_, _)
        | Literal::Undef
        | Literal::Expr { .. }
        | Literal::QuoteNode(_)
        | Literal::LineNumberNode { .. } => None,
    }
}

fn collect_declared_globals_in_testset_scope(
    block: &Block,
    out: &mut std::collections::HashSet<String>,
) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::Global { names, .. } => out.extend(names.iter().cloned()),
            Stmt::Expr { expr, .. } => collect_declared_globals_in_testset_expr(expr, out),
            Stmt::Block(block)
            | Stmt::Timed { body: block, .. }
            | Stmt::TestSet { body: block, .. }
            | Stmt::For { body: block, .. }
            | Stmt::ForEach { body: block, .. }
            | Stmt::ForEachTuple { body: block, .. }
            | Stmt::While { body: block, .. } => {
                collect_declared_globals_in_testset_scope(block, out);
            }
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_declared_globals_in_testset_scope(then_branch, out);
                if let Some(block) = else_branch {
                    collect_declared_globals_in_testset_scope(block, out);
                }
            }
            Stmt::Try {
                try_block,
                catch_block,
                else_block,
                finally_block,
                ..
            } => {
                collect_declared_globals_in_testset_scope(try_block, out);
                for block in [catch_block, else_block, finally_block]
                    .into_iter()
                    .flatten()
                {
                    collect_declared_globals_in_testset_scope(block, out);
                }
            }
            _ => {}
        }
    }
}

fn collect_declared_globals_in_testset_expr(
    expr: &Expr,
    out: &mut std::collections::HashSet<String>,
) {
    match expr {
        Expr::LetBlock { body, .. } => collect_declared_globals_in_testset_scope(body, out),
        Expr::Call { args, kwargs, .. } | Expr::ModuleCall { args, kwargs, .. } => {
            for arg in args {
                collect_declared_globals_in_testset_expr(arg, out);
            }
            for (_, value) in kwargs {
                collect_declared_globals_in_testset_expr(value, out);
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_declared_globals_in_testset_expr(left, out);
            collect_declared_globals_in_testset_expr(right, out);
        }
        Expr::UnaryOp { operand, .. } => collect_declared_globals_in_testset_expr(operand, out),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            collect_declared_globals_in_testset_expr(condition, out);
            collect_declared_globals_in_testset_expr(then_expr, out);
            collect_declared_globals_in_testset_expr(else_expr, out);
        }
        Expr::TupleLiteral { elements, .. } | Expr::ArrayLiteral { elements, .. } => {
            for elem in elements {
                collect_declared_globals_in_testset_expr(elem, out);
            }
        }
        _ => {}
    }
}

fn collect_let_body_assignment_names(block: &Block, out: &mut HashSet<String>) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::Assign { var, value, .. } | Stmt::AddAssign { var, value, .. } => {
                out.insert(var.to_string());
                collect_let_expr_assignment_names(value, out);
            }
            // Plain/timed blocks are scope-transparent. Loops, try clauses,
            // and testsets own separate Julia scopes and are deliberately not
            // descended here; their compiler paths create their own lexical
            // declaration owners (Issue #11569).
            Stmt::Block(block) | Stmt::Timed { body: block, .. } => {
                collect_let_body_assignment_names(block, out);
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                collect_let_expr_assignment_names(condition, out);
                collect_let_body_assignment_names(then_branch, out);
                if let Some(block) = else_branch {
                    collect_let_body_assignment_names(block, out);
                }
            }
            Stmt::Try { .. }
            | Stmt::TestSet { .. }
            | Stmt::For { .. }
            | Stmt::ForEach { .. }
            | Stmt::ForEachTuple { .. }
            | Stmt::While { .. } => {}
            Stmt::Expr { expr, .. } => collect_let_expr_assignment_names(expr, out),
            Stmt::Return {
                value: Some(expr), ..
            } => collect_let_expr_assignment_names(expr, out),
            Stmt::FunctionDef { func, .. } if is_lifted_generator_helper_name(&func.name) => {
                out.insert(func.name.clone());
            }
            Stmt::FunctionDef { .. } => {}
            Stmt::Global { .. } => {}
            _ => {}
        }
    }
}

fn is_lifted_generator_helper_name(name: &str) -> bool {
    let leaf = name.rsplit('#').next().unwrap_or(name);
    leaf.starts_with("__gen_body_") || leaf.starts_with("__gen_pred_")
}

fn collect_let_expr_assignment_names(expr: &Expr, out: &mut HashSet<String>) {
    match expr {
        Expr::AssignExpr { var, value, .. } => {
            out.insert(var.to_string());
            collect_let_expr_assignment_names(value, out);
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_let_expr_assignment_names(left, out);
            collect_let_expr_assignment_names(right, out);
        }
        Expr::UnaryOp { operand, .. } => collect_let_expr_assignment_names(operand, out),
        Expr::Call { args, kwargs, .. } | Expr::ModuleCall { args, kwargs, .. } => {
            for arg in args {
                collect_let_expr_assignment_names(arg, out);
            }
            for (_, value) in kwargs {
                collect_let_expr_assignment_names(value, out);
            }
        }
        Expr::Builtin { args, .. } => {
            for arg in args {
                collect_let_expr_assignment_names(arg, out);
            }
        }
        Expr::ArrayLiteral { elements, .. } | Expr::TupleLiteral { elements, .. } => {
            for element in elements {
                collect_let_expr_assignment_names(element, out);
            }
        }
        Expr::Index { array, indices, .. } => {
            collect_let_expr_assignment_names(array, out);
            for index in indices {
                collect_let_expr_assignment_names(index, out);
            }
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            collect_let_expr_assignment_names(start, out);
            if let Some(step) = step {
                collect_let_expr_assignment_names(step, out);
            }
            collect_let_expr_assignment_names(stop, out);
        }
        Expr::NamedTupleLiteral { fields, .. } => {
            for (_, value) in fields {
                collect_let_expr_assignment_names(value, out);
            }
        }
        Expr::Pair { key, value, .. } => {
            collect_let_expr_assignment_names(key, out);
            collect_let_expr_assignment_names(value, out);
        }
        Expr::DictLiteral { pairs, .. } => {
            for (key, value) in pairs {
                collect_let_expr_assignment_names(key, out);
                collect_let_expr_assignment_names(value, out);
            }
        }
        Expr::LetBlock { bindings, body, .. } => {
            for (_, value) in bindings {
                collect_let_expr_assignment_names(value, out);
            }
            // Only a scope-transparent `begin ... end` (empty bindings) shares
            // the enclosing owner. A real nested `let` and a macro-expanded
            // testset own separate scopes and are compiled by their own
            // LetBlock invocation (Issue #11569).
            if bindings.is_empty() {
                collect_let_body_assignment_names(body, out);
            }
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            collect_let_expr_assignment_names(condition, out);
            collect_let_expr_assignment_names(then_expr, out);
            collect_let_expr_assignment_names(else_expr, out);
        }
        Expr::StringConcat { parts, .. } => {
            for part in parts {
                collect_let_expr_assignment_names(part, out);
            }
        }
        _ => {}
    }
}

fn collect_let_expr_declared_globals(expr: &Expr, out: &mut HashSet<String>) {
    match expr {
        Expr::BinaryOp { left, right, .. } => {
            collect_let_expr_declared_globals(left, out);
            collect_let_expr_declared_globals(right, out);
        }
        Expr::UnaryOp { operand, .. } => collect_let_expr_declared_globals(operand, out),
        Expr::Call { args, kwargs, .. } | Expr::ModuleCall { args, kwargs, .. } => {
            for arg in args {
                collect_let_expr_declared_globals(arg, out);
            }
            for (_, value) in kwargs {
                collect_let_expr_declared_globals(value, out);
            }
        }
        Expr::Builtin { args, .. } => {
            for arg in args {
                collect_let_expr_declared_globals(arg, out);
            }
        }
        Expr::ArrayLiteral { elements, .. } | Expr::TupleLiteral { elements, .. } => {
            for element in elements {
                collect_let_expr_declared_globals(element, out);
            }
        }
        Expr::Index { array, indices, .. } => {
            collect_let_expr_declared_globals(array, out);
            for index in indices {
                collect_let_expr_declared_globals(index, out);
            }
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            collect_let_expr_declared_globals(start, out);
            if let Some(step) = step {
                collect_let_expr_declared_globals(step, out);
            }
            collect_let_expr_declared_globals(stop, out);
        }
        Expr::NamedTupleLiteral { fields, .. } => {
            for (_, value) in fields {
                collect_let_expr_declared_globals(value, out);
            }
        }
        Expr::Pair { key, value, .. } => {
            collect_let_expr_declared_globals(key, out);
            collect_let_expr_declared_globals(value, out);
        }
        Expr::DictLiteral { pairs, .. } => {
            for (key, value) in pairs {
                collect_let_expr_declared_globals(key, out);
                collect_let_expr_declared_globals(value, out);
            }
        }
        Expr::LetBlock { bindings, body, .. } => {
            for (_, value) in bindings {
                collect_let_expr_declared_globals(value, out);
            }
            if bindings.is_empty() {
                collect_let_body_declared_globals(body, out);
            }
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            collect_let_expr_declared_globals(condition, out);
            collect_let_expr_declared_globals(then_expr, out);
            collect_let_expr_declared_globals(else_expr, out);
        }
        Expr::StringConcat { parts, .. } => {
            for part in parts {
                collect_let_expr_declared_globals(part, out);
            }
        }
        Expr::AssignExpr { value, .. }
        | Expr::ReturnExpr {
            value: Some(value), ..
        } => {
            collect_let_expr_declared_globals(value, out);
        }
        _ => {}
    }
}

/// Collect names declared `global` anywhere in a `let` body, using the SAME
/// scope-descent rule as [`collect_let_body_assignment_names`] (into
/// scope-transparent `begin` and `if`, but NOT nested hard/soft scopes or
/// function bodies). A
/// `global x` binds the module global, so its name must be excluded from the
/// let-local set — neither shadow-saved nor forgotten at block exit (Issue
/// #9313). Kept in lockstep with `collect_let_body_assignment_names` so a
/// nested-block `global` is excluded wherever that pass would have collected an
/// assignment of the same name.
fn collect_let_body_declared_globals(block: &Block, out: &mut HashSet<String>) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::Global { names, .. } => out.extend(names.iter().cloned()),
            Stmt::Block(block) | Stmt::Timed { body: block, .. } => {
                collect_let_body_declared_globals(block, out);
            }
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_let_body_declared_globals(then_branch, out);
                if let Some(block) = else_branch {
                    collect_let_body_declared_globals(block, out);
                }
            }
            Stmt::Try { .. }
            | Stmt::TestSet { .. }
            | Stmt::For { .. }
            | Stmt::ForEach { .. }
            | Stmt::ForEachTuple { .. }
            | Stmt::While { .. } => {}
            Stmt::Expr {
                expr: Expr::LetBlock { bindings, body, .. },
                ..
            } if bindings.is_empty() => {
                collect_let_body_declared_globals(body, out);
            }
            _ => {}
        }
    }
}

fn array_literal_element_ranks(elements: &[Expr]) -> Option<Vec<usize>> {
    elements
        .iter()
        .map(|element| match element {
            Expr::ArrayLiteral { shape, .. } => Some(shape.len()),
            _ => None,
        })
        .collect()
}

fn tuple_field_array_element_type(value_type: &ValueType) -> ArrayElementType {
    match value_type {
        ValueType::I8 => ArrayElementType::I8,
        ValueType::I16 => ArrayElementType::I16,
        ValueType::I32 => ArrayElementType::I32,
        ValueType::I64 => ArrayElementType::I64,
        ValueType::U8 => ArrayElementType::U8,
        ValueType::U16 => ArrayElementType::U16,
        ValueType::U32 => ArrayElementType::U32,
        ValueType::U64 => ArrayElementType::U64,
        // Issue #9301: Float16 has a boxed storage tag (mirrors I128/U128), so a
        // tuple field of type Float16 must narrow like F32/F64, not widen to Any.
        ValueType::F16 => ArrayElementType::F16,
        ValueType::F32 => ArrayElementType::F32,
        ValueType::F64 => ArrayElementType::F64,
        ValueType::Bool => ArrayElementType::Bool,
        ValueType::Str => ArrayElementType::String,
        ValueType::Char => ArrayElementType::Char,
        ValueType::Symbol => ArrayElementType::Symbol,
        _ => ArrayElementType::Any,
    }
}

/// Preserve a concrete *parametric* element type written as a `T[]` literal
/// (`UnitRange{Int64}[]`, `Vector{Int}[]`, ...) instead of widening it to `Any`
/// (Issue #6768). Returns `Some(Abstract(name))` when `type_name` is a
/// parameterized type whose every component is a type name (uppercase-initial),
/// i.e. it carries no free type variable; otherwise `None` so the caller keeps
/// the legacy `Any` fallback.
///
/// Concrete-storage parametric eltypes (`Complex{Float64}`) and registered
/// structs are handled by earlier arms before this fallback is consulted.
pub(in crate::compile::expr) fn concrete_parametric_element_type_from_name(
    type_name: &str,
) -> Option<ArrayElementType> {
    // Must be a parametric form `Base{...}` with a non-empty parameter list.
    let open = type_name.find('{')?;
    if !type_name.ends_with('}') || open + 1 >= type_name.len() - 1 {
        return None;
    }
    // Every identifier token (base + each parameter component) must look like a
    // concrete type name (starts uppercase), so we never preserve a free type
    // variable such as `UnitRange{T}` written inside a `where` body.
    let mut ident = String::new();
    for ch in type_name.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            ident.push(ch);
        } else {
            if !is_concrete_type_ident(&ident) {
                return None;
            }
            ident.clear();
        }
    }
    if !is_concrete_type_ident(&ident) {
        return None;
    }
    Some(ArrayElementType::Abstract(type_name.to_string()))
}

/// An identifier inside a parametric type name is "concrete" when it is empty
/// (a separator boundary already validated by its neighbours) or begins with an
/// uppercase letter (a type name, not a lowercase type variable).
fn is_concrete_type_ident(ident: &str) -> bool {
    ident
        .chars()
        .next()
        .is_none_or(|c| c.is_uppercase() || c.is_ascii_digit())
}

impl CoreCompiler<'_> {
    fn tuple_literal_array_element_type(&mut self, elements: &[Expr]) -> Option<ArrayElementType> {
        let mut tuple_fields: Option<Vec<ArrayElementType>> = None;
        for element in elements {
            let Expr::TupleLiteral {
                elements: fields, ..
            } = element
            else {
                return None;
            };
            let field_types: Vec<ArrayElementType> = fields
                .iter()
                .map(|field| tuple_field_array_element_type(&self.infer_expr_type(field)))
                .collect();
            match &tuple_fields {
                Some(existing) if existing != &field_types => return None,
                Some(_) => {}
                None => tuple_fields = Some(field_types),
            }
        }
        tuple_fields.map(ArrayElementType::TupleOf)
    }

    pub(in crate::compile) fn is_array_wrapper_value_type(&self, ty: &ValueType) -> bool {
        matches!(ty, ValueType::Struct(type_id)
        if self.shared_ctx.get_struct_name(*type_id).is_some_and(|name| {
            name == "Array"
                || name.starts_with("Array{")
                || name.starts_with("Vector{")
                || name.starts_with("Matrix{")
        }))
    }

    pub(in crate::compile::expr) fn emit_array_wrapper_memory_start(
        &mut self,
        elem_type: ArrayElementType,
        len: usize,
    ) {
        // Build the backing `Memory{T}` directly. The finalize step
        // (`emit_array_wrapper_from_memory_on_stack`) wraps it into the
        // `Array{T,N}` natively, so we no longer push the `Array` `DataType`
        // that the old pure-Julia `wrap(::Type{Array}, ...)` call consumed
        // (Issue #6846).
        self.emit(Instr::NewMemory(elem_type, len));
    }

    pub(in crate::compile::expr) fn emit_array_wrapper_from_memory_on_stack(
        &mut self,
        shape: &[usize],
    ) {
        // Wrap the `Memory{T}` on top of the stack into the `Array{T,N}`
        // wrapper with a native `FinalizeArray` instead of a per-literal
        // pure-Julia `wrap(::Type{Array}, mem, dims)` call. `wrap` spun up
        // ~5 Julia frames (`wrap` → `_array_wrap_check` → `memoryref` →
        // `_array_construct` → `Array{T,N}(ref, dims)`) for every array
        // literal, which dominated tight allocation loops such as
        // `(x, y) -> sinc(norm([x, y]))` over a grid (Issue #6846). The
        // `FinalizeArray` handler reconstructs the exact same wrapper from the
        // `Memory` build buffer (shared with the comprehension build path,
        // Issue #6807) with no Julia frame.
        self.emit(Instr::FinalizeArray(shape.to_vec()));
    }

    pub(in crate::compile::expr) fn emit_empty_array_wrapper(
        &mut self,
        elem_type: ArrayElementType,
        shape: &[usize],
    ) {
        let len = shape.iter().product();
        self.emit_array_wrapper_memory_start(elem_type, len);
        self.emit_array_wrapper_from_memory_on_stack(shape);
    }

    /// Compile one element of an inline `Complex{Float64}` / `Complex{Float32}`
    /// array literal, coercing it to the array's storage type (Issue #6867).
    ///
    /// The element may be:
    /// - already the exact inline complex type (`target`) → pushed as-is;
    /// - a `Complex{...}` value of a different parameter (e.g. `Complex{Float32}`
    ///   into a `Complex{Float64}` array, from a `Complex×Real` promotion) →
    ///   rebuilt via `target_name(real(z), imag(z))`;
    /// - a real numeric (`Float64`, `Int64`, ...) → widened via
    ///   `target_name(x, 0)`, mirroring `promote_type(Complex{T}, Real)`.
    fn compile_complex_array_element(
        &mut self,
        elem: &Expr,
        elem_type: &ValueType,
        target: ValueType,
        target_name: &str,
    ) -> CResult<()> {
        // Exact inline complex value: store directly.
        if *elem_type == target {
            self.compile_expr(elem)?;
            return Ok(());
        }
        // A struct-backed `Complex{T}` whose name matches the target storage is
        // representation-compatible; store directly (existing fast path).
        if let ValueType::Struct(id) = elem_type {
            if self.shared_ctx.get_struct_name(*id).as_deref() == Some(target_name) {
                self.compile_expr(elem)?;
                return Ok(());
            }
        }

        let span = elem.span();
        let is_complex_elem = matches!(elem_type, ValueType::ComplexF32 | ValueType::ComplexF64)
            || self.is_struct_type_of(elem_type.clone(), "Complex");

        let args = if is_complex_elem {
            // Convert a differently-parameterized Complex via real/imag parts.
            let real_call = Expr::Call {
                function: "real".to_string().into(),
                args: vec![elem.clone()],
                kwargs: Vec::new(),
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span,
            };
            let imag_call = Expr::Call {
                function: "imag".to_string().into(),
                args: vec![elem.clone()],
                kwargs: Vec::new(),
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span,
            };
            vec![real_call, imag_call]
        } else {
            // Real numeric element: imaginary part is zero.
            vec![elem.clone(), Expr::Literal(Literal::Int(0), span)]
        };

        let complex_call = Expr::Call {
            function: target_name.to_string().into(),
            args,
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        };
        self.compile_expr_as(&complex_call, target)
    }

    /// Exact private-helper family denoted by a lowered FunctionRef. A source
    /// span is part of helper identity because its spelling may legally collide
    /// with a Julia-visible user generic (Issue #9784).
    pub(in super::super) fn lowering_helper_function_ref_candidates(
        &self,
        expr: &Expr,
    ) -> Vec<usize> {
        let Expr::FunctionRef { name, span } = expr else {
            return Vec::new();
        };
        self.shared_ctx
            .function_indices_by_span_start
            .get(&span.start)
            .into_iter()
            .flatten()
            .filter(|index| {
                self.shared_ctx
                    .function_ir_by_global_index
                    .get(index)
                    .is_some_and(|function| {
                        function.name == name.as_str()
                            && crate::compile::ir_inline::is_markerless_lowered_function(function)
                    })
            })
            .copied()
            .collect()
    }

    pub(super) fn compile_expr(&mut self, expr: &Expr) -> CResult<ValueType> {
        self.set_current_span(expr.span());
        match expr {
            Expr::Literal(lit, span) => match lit {
                Literal::Int(v) => {
                    self.emit(Instr::PushI64(*v));
                    Ok(ValueType::I64)
                }
                Literal::Int128(v) => {
                    self.emit(Instr::PushI128(Box::new(*v)));
                    Ok(ValueType::I128)
                }
                Literal::BigInt(s) => {
                    self.emit(Instr::PushBigInt(s.clone()));
                    Ok(ValueType::BigInt)
                }
                Literal::BigFloat(s) => {
                    self.emit(Instr::PushBigFloat(s.clone()));
                    Ok(ValueType::BigFloat)
                }
                Literal::Float(v) => {
                    self.emit(Instr::PushF64(*v));
                    Ok(ValueType::F64)
                }
                Literal::Float32(v) => {
                    self.emit(Instr::PushF32(*v));
                    Ok(ValueType::F32)
                }
                Literal::Float16(v) => {
                    self.emit(Instr::PushF16(*v));
                    Ok(ValueType::F16)
                }
                Literal::Bool(b) => {
                    self.emit(Instr::PushBool(*b));
                    Ok(ValueType::Bool)
                }
                Literal::Str(s) => {
                    self.emit(Instr::PushStr(s.clone()));
                    Ok(ValueType::Str)
                }
                Literal::StrBytes(bytes) => {
                    self.emit(Instr::PushStrBytes(bytes.clone()));
                    Ok(ValueType::Str)
                }
                Literal::Char(c) => {
                    self.emit(Instr::PushChar(*c));
                    Ok(ValueType::Char)
                }
                Literal::CharMalformed(bits) => {
                    self.emit(Instr::PushCharMalformed(*bits));
                    Ok(ValueType::Char)
                }
                Literal::Nothing => {
                    self.emit(Instr::PushNothing);
                    Ok(ValueType::Nothing)
                }
                Literal::Missing => {
                    self.emit(Instr::PushMissing);
                    Ok(ValueType::Missing)
                }
                Literal::Array(data, shape) => {
                    self.emit(Instr::PushArrayValue(Box::new(ArrayLiteralPayload::F64 {
                        data: data.clone(),
                        shape: shape.clone(),
                    })));
                    Ok(ValueType::ArrayOf(ArrayElementType::F64, None))
                }
                Literal::ArrayI64(data, shape) => {
                    self.emit(Instr::PushArrayValue(Box::new(ArrayLiteralPayload::I64 {
                        data: data.clone(),
                        shape: shape.clone(),
                    })));
                    Ok(ValueType::ArrayOf(ArrayElementType::I64, None))
                }
                Literal::ArrayBool(data, shape) => {
                    self.emit(Instr::PushArrayValue(Box::new(ArrayLiteralPayload::Bool {
                        data: data.clone(),
                        shape: shape.clone(),
                    })));
                    Ok(ValueType::ArrayOf(ArrayElementType::Bool, None))
                }
                Literal::Struct(type_name, field_literals) => {
                    // Look up struct info by name
                    let struct_info =
                        self.shared_ctx.struct_table.get(type_name).ok_or_else(|| {
                            CompileError::Msg(format!("Unknown struct type: {}", type_name))
                        })?;
                    let type_id = struct_info.type_id;
                    let expected_field_count = struct_info.fields.len();
                    let field_types: Vec<ValueType> = struct_info
                        .fields
                        .iter()
                        .map(|(_, ty)| ty.clone())
                        .collect();

                    if field_literals.len() != expected_field_count {
                        return err(format!(
                            "Struct {} expects {} fields, got {}",
                            type_name,
                            expected_field_count,
                            field_literals.len()
                        ));
                    }

                    // Compile each field literal with the expected type
                    for (literal, expected_ty) in field_literals.iter().zip(field_types.iter()) {
                        let literal_expr = Expr::Literal(literal.clone(), *span);
                        self.compile_expr_as(&literal_expr, expected_ty.clone())?;
                    }

                    // Emit NewStruct instruction
                    self.emit(Instr::NewStruct(type_id, field_literals.len()));
                    Ok(ValueType::Struct(type_id))
                }
                Literal::Module(name) => {
                    // Source identifiers lower to module literals only for
                    // builtin roots. They still need lexical visibility
                    // checks (notably baremodule's lack of implicit Base).
                    // Other module literals are compiler-generated identity
                    // values (`__module__`, macro call-site arguments, or a
                    // runtime Module converted back to IR). Re-resolving those
                    // as source names breaks macro expansion before the
                    // enclosing module has been activated at runtime.
                    let module_name = if crate::module_names::is_builtin_literal_root(name) {
                        let Some(module_name) = self.resolve_visible_module_path(name) else {
                            return Ok(self.emit_unbound_module_name(name));
                        };
                        module_name
                    } else {
                        self.canonical_module_path(name)
                    };
                    let export_key = module_name.as_str();
                    let exports = self
                        .module_exports
                        .get(export_key)
                        .map(|set| {
                            let mut exports: Vec<String> = set.iter().cloned().collect();
                            exports.sort();
                            exports
                        })
                        .unwrap_or_default();
                    self.emit(Instr::PushModule(Box::new(ModuleOperands {
                        name: module_name,
                        exports,
                        publics: vec![],
                        base_exports_visible: true,
                        implicit_standard_bindings: true,
                    })));
                    Ok(ValueType::Module)
                }
                Literal::DataType(name) => {
                    // Static parametric type expressions lower to a DataType
                    // literal rather than `Expr::Var`. They must consult the
                    // same lexical Core/Base authority as every other type
                    // consumer; otherwise `Vector{Int}` bypasses a
                    // baremodule's missing Base binding (Issue #11419).
                    if let Some(hidden_name) = self.first_hidden_builtin_type_binding(name) {
                        return Ok(self.emit_unbound_module_name(&hidden_name));
                    }
                    // A literal `Base{...}` on a builtin base that declares no
                    // type parameters (`Int64{Float64}`, `Real{Int64}`) is not
                    // a `UnionAll` application: route it through the runtime
                    // `ApplyTypeDynamic` validator — the same path
                    // `Core.apply_type` uses — so it raises the upstream
                    // `TypeError` instead of pushing a fabricated nonsense
                    // `DataType` (Issue #10654).
                    if let Some((base, type_args)) = literal_non_parametric_type_application(name) {
                        self.emit(Instr::PushDataType(base.to_string()));
                        let num_type_args = type_args.len();
                        for arg in type_args {
                            self.emit(Instr::PushDataType(arg));
                        }
                        self.emit(Instr::ApplyTypeDynamic(num_type_args));
                        return Ok(ValueType::DataType);
                    }
                    self.emit(Instr::PushDataType(name.clone()));
                    Ok(ValueType::DataType)
                }
                Literal::Undef => {
                    // Undef is used for required keyword arguments (no default value)
                    self.emit(Instr::PushUndef);
                    Ok(ValueType::Any)
                }
                // Metaprogramming literals (for REPL persistence) and
                // macro-injected `QuoteNode(:sym)` arguments (Issue #7163). The
                // emitted `PushSymbol` produces a genuine `Value::Symbol`, so the
                // static type must be `Symbol` (not `Any`) to match a `::Symbol`
                // field/parameter slot — otherwise the constructor field coercion
                // sees `Any` and errors with "Cannot convert Any to Symbol".
                // Mirrors the source-level `:sym` path (`QuoteLiteral(SymbolNew)`,
                // which already reports `ValueType::Symbol`) and the literal-type
                // inference functions (`infer_expr_type`, `literal_rhs_value_type`,
                // `infer_default_type`).
                Literal::Symbol(name) => {
                    self.emit(Instr::PushSymbol(name.clone()));
                    Ok(ValueType::Symbol)
                }
                Literal::Expr { head, args } => {
                    // Compile each arg literal first (they will be pushed on stack)
                    for arg in args {
                        let arg_expr = Expr::Literal(arg.clone(), *span);
                        self.compile_expr(&arg_expr)?;
                    }
                    // Emit CreateExpr to pop args and create Expr value
                    self.emit(Instr::CreateExpr {
                        head: head.clone(),
                        arg_count: args.len(),
                    });
                    Ok(ValueType::Any)
                }
                Literal::QuoteNode(inner) => {
                    // Compile the inner literal
                    let inner_expr = Expr::Literal(inner.as_ref().clone(), *span);
                    self.compile_expr(&inner_expr)?;
                    // Wrap in QuoteNode
                    self.emit(Instr::CreateQuoteNode);
                    Ok(ValueType::Any)
                }
                Literal::LineNumberNode { line, file } => {
                    self.emit(Instr::PushLineNumberNode {
                        line: *line,
                        file: file.clone(),
                    });
                    Ok(ValueType::Any)
                }
                Literal::Regex { pattern, flags } => {
                    self.emit(Instr::PushRegex {
                        pattern: pattern.clone(),
                        flags: flags.clone(),
                    });
                    Ok(ValueType::Regex)
                }
                Literal::Enum { type_name, value } => {
                    self.emit(Instr::PushEnum {
                        type_name: type_name.clone(),
                        value: *value,
                    });
                    Ok(ValueType::Enum)
                }
            },
            Expr::Var(name, span) => {
                let lexical_local_shadows_alias = self.explicit_lexical_owner_active(name)
                    || ((self.strict_undefined_check || self.local_scope_depth > 0)
                        && (self.locals.contains_key(name.as_str())
                            || self.captured_vars.contains(name.as_str())));
                let runtime_imported_binding = !self.in_base_function_scope
                    && self.imported_bindings.contains(name.as_str())
                    && (self.imported_binding_has_non_base_source(name)
                        || self.imported_binding_is_renamed(name))
                    && self.module_path_in_current_scope(name).is_none()
                    && self
                        .current_module_path
                        .as_ref()
                        .is_none_or(|scope| !self.module_has_binding(scope, name));
                if !lexical_local_shadows_alias && runtime_imported_binding {
                    let value_type = if self.resolved_active_imported_type_name(name).is_some() {
                        ValueType::DataType
                    } else {
                        ValueType::Any
                    };
                    self.emit_load_imported_binding(name);
                    return Ok(value_type);
                }
                if !self.in_base_function_scope
                    && !lexical_local_shadows_alias
                    && self.module_alias_states.contains_key(name.as_str())
                {
                    match self.module_alias_states.get(name.as_str()).cloned() {
                        Some(crate::compile::core_compiler::ModuleAliasState::Bound {
                            canonical_target,
                            kind: crate::compile::core_compiler::ImportBindingKind::Module,
                            ..
                        }) => {
                            self.emit_module_value(&canonical_target);
                            return Ok(ValueType::Module);
                        }
                        Some(crate::compile::core_compiler::ModuleAliasState::Bound {
                            canonical_target,
                            kind: crate::compile::core_compiler::ImportBindingKind::NonModule,
                            ..
                        }) => {
                            if let Some((module, binding)) = canonical_target.rsplit_once('.') {
                                return self.compile_resolved_module_function_ref(module, binding);
                            }
                            return self.compile_expr(&Expr::Var(canonical_target.into(), *span));
                        }
                        Some(crate::compile::core_compiler::ModuleAliasState::Ambiguous) => {
                            // An ambiguous nonselective export is a lexical name
                            // with no binding, not permission to pick a provider.
                            if self.imported_bindings.contains(name.as_str()) {
                                self.emit_load_imported_binding(name);
                                return Ok(ValueType::Any);
                            }
                            return Ok(self.emit_unbound_module_name(name));
                        }
                        None => {}
                    }
                }
                // A whole-scope `global x` declaration outranks a clause-local
                // type entry left by the inference pre-scan. Resolve it through
                // `load_local` (which emits `LoadGlobalAny`) before the
                // uninitialized-local path can emit a slotizable `LoadAny`.
                // Explicit locals in a hard clause temporarily remove the name
                // from `declared_globals`, so their reads still take the local
                // path (Issues #5548, #5549, #11281).
                if self.declared_globals.contains(name.as_str()) {
                    self.load_local(name)?;
                    return Ok(ValueType::Any);
                }

                if lexical_local_shadows_alias
                    && self.locals.contains_key(name.as_str())
                    && !self.initialized_locals.contains(name.as_str())
                    && !self.captured_vars.contains(name.as_str())
                {
                    self.emit(Instr::LoadAny(name.to_string()));
                    return Ok(ValueType::Any);
                }
                if self.is_renamed_only_module_root(name) {
                    return Ok(self.emit_unbound_module_name(name));
                }
                if name == "nothing" && !self.locals.contains_key(name.as_str()) {
                    self.emit(Instr::PushNothing);
                    return Ok(ValueType::Nothing);
                }

                // A captured closure variable shadows any same-named Base
                // function, type name, or module alias — exactly as a plain
                // local does. Resolve it through `load_local` (which emits
                // `LoadCaptured`) BEFORE the Base-function / type-name checks
                // below; otherwise a captured accumulator whose name collides
                // with a `Base` function (e.g. `count`, `sum`) would compile to
                // `PushFunction("count")` and the closure body would operate on
                // the `Base` function value instead of the captured local
                // (Issue #7619).
                if self.captured_vars.contains(name.as_str())
                    && !self.locals.contains_key(name.as_str())
                {
                    self.load_local(name)?;
                    return Ok(ValueType::Any);
                }

                // Check if this is a type parameter from a where clause
                // Type parameters are resolved at runtime
                if self.current_type_param_index.contains_key(name.as_str()) {
                    // Check if this is a Val{N} type parameter - these are values (int/bool/symbol), not types
                    if self.val_type_params.contains(name.as_str())
                        || self.val_bool_params.contains(name.as_str())
                        || self.val_symbol_params.contains(name.as_str())
                    {
                        // Val type parameters are stored in specialized maps at runtime
                        // Use LoadAny to check all possible storages (i64, bool, symbol)
                        self.emit(Instr::LoadAny(name.to_string()));
                        return Ok(ValueType::Any);
                    }
                    // Regular type parameters are resolved via LoadTypeBinding
                    self.emit(Instr::LoadTypeBinding(name.to_string()));
                    return Ok(ValueType::DataType);
                }

                // Preserve the established implicit-Base import path. User and
                // renamed imports that need source ordering returned through
                // `runtime_imported_binding` above; ordinary Base bindings keep
                // their active type/value representation for inference-heavy
                // Base helpers such as collect/similar.
                if !self.locals.contains_key(name.as_str())
                    && !self.captured_vars.contains(name.as_str())
                    && self.imported_bindings.contains(name.as_str())
                    && self.module_path_in_current_scope(name).is_none()
                    && self
                        .current_module_path
                        .as_ref()
                        .is_none_or(|scope| !self.module_has_binding(scope, name))
                {
                    if let Some(type_name) = self.resolved_active_imported_type_name(name) {
                        self.emit(Instr::PushDataType(type_name));
                        return Ok(ValueType::DataType);
                    }
                    self.emit_load_imported_binding(name);
                    return Ok(ValueType::Any);
                }

                // Handle pi/π, NaN, Inf constants (always available without imports).
                // Built-in irrational singletons are also recorded in global_const_structs;
                // preserve those bindings when present instead of lowering the variable
                // reference directly to a Float64 literal (Issue #8481).
                if !self.locals.contains_key(name.as_str()) {
                    if let Some(ty) = self.emit_builtin_irrational_singleton(name) {
                        return Ok(ty);
                    }
                    if is_pi_name(name) {
                        self.emit(Instr::PushF64(std::f64::consts::PI));
                        return Ok(ValueType::F64);
                    }
                    if is_euler_name(name) {
                        self.emit(Instr::PushF64(std::f64::consts::E));
                        return Ok(ValueType::F64);
                    }
                    if name == "NaN" {
                        self.emit(Instr::PushF64(f64::NAN));
                        return Ok(ValueType::F64);
                    }
                    if name == "Inf" {
                        self.emit(Instr::PushF64(f64::INFINITY));
                        return Ok(ValueType::F64);
                    }
                    // Handle Float32 special values
                    if name == "Inf32" {
                        self.emit(Instr::PushF32(f32::INFINITY));
                        return Ok(ValueType::F32);
                    }
                    if name == "NaN32" {
                        self.emit(Instr::PushF32(f32::NAN));
                        return Ok(ValueType::F32);
                    }
                    // Handle Float16 special values
                    if name == "Inf16" {
                        self.emit(Instr::PushF16(f16::INFINITY));
                        return Ok(ValueType::F16);
                    }
                    if name == "NaN16" {
                        self.emit(Instr::PushF16(f16::NAN));
                        return Ok(ValueType::F16);
                    }
                    // Handle explicit Float64 special value aliases
                    if name == "Inf64" {
                        self.emit(Instr::PushF64(f64::INFINITY));
                        return Ok(ValueType::F64);
                    }
                    if name == "NaN64" {
                        self.emit(Instr::PushF64(f64::NAN));
                        return Ok(ValueType::F64);
                    }
                    // Handle Julia global constants: ARGS, PROGRAM_FILE
                    // Note: VERSION is defined in version.jl as a VersionNumber struct,
                    // not handled as a special case here.
                    if name == "ARGS" {
                        // ARGS is an empty String array (command-line args not passed through)
                        self.emit(Instr::NewArrayTyped(ArrayElementType::String, 0));
                        self.emit(Instr::FinalizeArrayTyped(vec![0]));
                        return Ok(ValueType::ArrayOf(ArrayElementType::String, None));
                    }
                    if name == "PROGRAM_FILE" {
                        // PROGRAM_FILE is empty string when in REPL/embedded mode
                        self.emit(Instr::PushStr(String::new()));
                        return Ok(ValueType::Str);
                    }
                    if name == "ENDIAN_BOM" {
                        // ENDIAN_BOM: 32-bit byte-order-mark indicating native byte order
                        // Little-endian: 0x04030201, Big-endian: 0x01020304
                        // Most modern systems are little-endian
                        #[cfg(target_endian = "little")]
                        let bom: i64 = 0x04030201;
                        #[cfg(target_endian = "big")]
                        let bom: i64 = 0x01020304;
                        self.emit(Instr::PushI64(bom));
                        return Ok(ValueType::I64);
                    }
                    // Standard IO streams. A keyword/local binding named
                    // stdout/stderr/stdin must shadow these globals (Issue #10034).
                    if !self.initialized_locals.contains(name.as_str()) {
                        if name == "stdout" {
                            self.emit(Instr::PushStdout);
                            return Ok(ValueType::IO);
                        }
                        if name == "stderr" {
                            self.emit(Instr::PushStderr);
                            return Ok(ValueType::IO);
                        }
                        if name == "stdin" {
                            self.emit(Instr::PushStdin);
                            return Ok(ValueType::IO);
                        }
                    }
                    if name == "devnull" {
                        self.emit(Instr::PushDevnull);
                        return Ok(ValueType::IO);
                    }
                    // C_NULL: Null pointer constant (Ptr{Cvoid}(0))
                    if name == "C_NULL" {
                        self.emit(Instr::PushCNull);
                        return Ok(ValueType::I64);
                    }
                    // DEPOT_PATH: Array of depot paths (empty in SubsetJuliaVM)
                    if name == "DEPOT_PATH" {
                        self.emit(Instr::NewArrayTyped(ArrayElementType::String, 0));
                        self.emit(Instr::FinalizeArrayTyped(vec![0]));
                        return Ok(ValueType::ArrayOf(ArrayElementType::String, None));
                    }
                    // LOAD_PATH: Array of load paths (empty in SubsetJuliaVM)
                    if name == "LOAD_PATH" {
                        self.emit(Instr::NewArrayTyped(ArrayElementType::String, 0));
                        self.emit(Instr::FinalizeArrayTyped(vec![0]));
                        return Ok(ValueType::ArrayOf(ArrayElementType::String, None));
                    }
                    // ENV: Environment variable dictionary (read-only
                    // Dict{String,String}). PushEnv supplies the raw OS pairs as
                    // a tuple of `(key, value)` 2-tuples; the pure-Julia
                    // `_env_from_pairs` helper builds the `Dict{String,String}`
                    // struct via the ordinary constructor, so ENV is a pure
                    // `Dict{K,V}` StructRef with no `Value::Dict` carrier
                    // (Issue #6731).
                    if name == "ENV" {
                        self.emit(Instr::PushEnv);
                        let candidates = self.runtime_candidates_for_names(&["_env_from_pairs"], 1);
                        if let Some(&fallback) = candidates.first() {
                            self.emit(Instr::CallTypedDispatch(
                                "_env_from_pairs".to_string(),
                                1,
                                fallback,
                                candidates,
                            ));
                        }
                        return Ok(ValueType::Any);
                    }
                }
                // Handle type names - push as DataType values for proper Julia semantics
                // Type names like Int64, Float64 are first-class values of type DataType
                if !self.locals.contains_key(name.as_str()) {
                    // A whole-program type registry entry supplies nominal
                    // identity, not a lexical binding. Resolve a hidden
                    // parent/sibling source type through the live module so a
                    // missing name raises the catchable UndefVarError before a
                    // global bare alias can leak into this scope. Keep the
                    // parametric callee evaluation order when an eval-created
                    // local binding exists (Issue #11168).
                    if let Some((base, type_args)) = parse_parametric_call(name) {
                        if self.source_type_binding_is_hidden(&base) {
                            self.emit_unbound_module_name(&base);
                            for type_arg in &type_args {
                                self.emit_parametric_type_arg_value(type_arg)?;
                            }
                            self.emit(Instr::ApplyTypeDynamic(type_args.len()));
                            return Ok(ValueType::DataType);
                        }
                    } else if self.source_type_binding_is_hidden(name) {
                        return Ok(self.emit_unbound_module_name(name));
                    }
                    // A registered builtin spelling is a type representation,
                    // not proof that its Core/Base binding is visible in this
                    // lexical module (Issue #11419).
                    if let Some(hidden_name) = self.first_hidden_builtin_type_binding(name) {
                        return Ok(self.emit_unbound_module_name(&hidden_name));
                    }
                    // A module value binding that shadowed an ignored
                    // conflicting import keeps the name a runtime value; read
                    // the module global instead of resolving the imported
                    // type statically (Issue #11426).
                    if let Some(qualified) = self.conflict_winning_module_value_binding(name) {
                        if !self.shared_ctx.type_aliases.contains_key(&qualified) {
                            self.emit(Instr::LoadGlobalAny(qualified));
                            return Ok(ValueType::Any);
                        }
                    }
                    // Check if it's a type alias (const MyInt = Int64) only
                    // after builtin ownership has been proven. The alias table
                    // retains global short keys that are not lexical bindings.
                    if let Some(target_type) = self.resolve_visible_type_alias(name) {
                        self.emit(Instr::PushDataType(target_type));
                        return Ok(ValueType::DataType);
                    }
                    // Emit the canonical nominal projection owned by the shared
                    // builtin type registry only after lexical ownership has
                    // been proven above (Issues #10954 and #11419).
                    if let Some(builtin_type) = crate::types::builtin_type_for_compiler(name) {
                        self.emit(Instr::PushDataType(builtin_type.name().into_owned()));
                        return Ok(ValueType::DataType);
                    }
                    if let Some(type_name) = self.resolve_visible_type_object_name(name) {
                        self.emit(Instr::PushDataType(type_name));
                        return Ok(ValueType::DataType);
                    }
                    // Dynamic Union spellings are grammar, not exact registry
                    // entries; preserve their complete nominal expression.
                    if is_builtin_type_name(name) {
                        self.emit(Instr::PushDataType(name.to_string()));
                        return Ok(ValueType::DataType);
                    }
                }
                // Whole-block inference pre-seeds `locals` with registry/global
                // slots that are not lexical bindings in this scope. Only an
                // initialized/captured value shadows module-name resolution;
                // otherwise an unrelated module slot could bypass the lexical
                // resolver (`import P as D` accidentally kept bare `P` visible,
                // Issue #11157).
                let has_lexical_value_binding = self.explicit_lexical_owner_active(name)
                    || if !self.strict_undefined_check {
                        self.current_module_path
                            .as_deref()
                            .and_then(|path| self.module_constants.get(path))
                            .is_some_and(|constants| constants.contains(name.as_str()))
                            || (self.current_module_path.is_none()
                                && self.initialized_locals.contains(name.as_str()))
                    } else {
                        self.initialized_locals.contains(name.as_str())
                            || self.captured_vars.contains(name.as_str())
                    };
                let has_lexical_value_binding =
                    has_lexical_value_binding && !self.is_renamed_only_module_root(name);
                if !has_lexical_value_binding {
                    if let Some(module_path) = self.resolve_visible_module_path(name) {
                        self.emit_module_value(&module_path);
                        return Ok(ValueType::Module);
                    }
                    if self.is_ambiguous_module_alias_root(name) {
                        return Ok(self.emit_unbound_module_name(name));
                    }
                    let canonical = self.resolve_module_alias_path(name);
                    if self.is_known_module_path(&canonical)
                        || self.is_known_module_short_name(name)
                    {
                        return Ok(self.emit_unbound_module_name(name));
                    }
                }
                if !self.locals.contains_key(name.as_str()) {
                    for using_module in self.visible_using_modules_for_name(name) {
                        if self.module_exports.contains_key(name.as_str())
                            || self.module_functions.contains_key(name.as_str())
                        {
                            let exports = self
                                .module_exports
                                .get(name.as_str())
                                .map(|set| {
                                    let mut exports: Vec<String> = set.iter().cloned().collect();
                                    exports.sort();
                                    exports
                                })
                                .unwrap_or_default();
                            self.emit(Instr::PushModule(Box::new(ModuleOperands {
                                name: name.to_string(),
                                exports,
                                publics: vec![],
                                base_exports_visible: true,
                                implicit_standard_bindings: true,
                            })));
                            return Ok(ValueType::Module);
                        }
                        let is_module_constant = self
                            .module_constants
                            .get(using_module.as_str())
                            .is_some_and(|constants| constants.contains(name.as_str()));
                        let qualified = format!("{}.{}", using_module, name);
                        let is_function = self.method_tables.contains_key(name.as_str())
                            || self.method_tables.contains_key(&qualified)
                            || is_base_function(name);
                        if is_module_constant || !is_function {
                            self.emit(Instr::LoadGlobalAny(qualified));
                            return Ok(ValueType::Any);
                        }
                    }
                }
                // Resolve bare function names to function objects when not a local variable
                if !self.locals.contains_key(name.as_str()) {
                    if let Some(qualified) = self.module_constant_qualified_name(name) {
                        self.emit(Instr::LoadGlobalAny(qualified));
                        return Ok(ValueType::Any);
                    }
                    if self.method_tables.contains_key(name.as_str())
                        && !self.hidden_user_globals.contains(name.as_str())
                    {
                        if !self.imported_functions.contains(name.as_str()) {
                            return err(format!(
                                "function '{}' is not imported. Use 'using ModuleName' or 'using ModuleName: {}' to import it, or use 'ModuleName.{}()' for qualified access.",
                                name, name, name
                            ));
                        }
                        self.emit_function_value(name);
                        return Ok(ValueType::Function);
                    }
                    if is_base_function(name) {
                        self.emit_function_value(name);
                        return Ok(ValueType::Function);
                    }
                    if self.usings.contains("Random") && is_random_function(name) {
                        self.emit(Instr::PushFunction(format!("Random.{}", name)));
                        return Ok(ValueType::Function);
                    }
                    // Handle MathConstants when imported via `using Base.MathConstants`
                    if self.usings.contains("Base.MathConstants") {
                        if let Some(value) = get_math_constant_value(name) {
                            self.emit(Instr::PushF64(value));
                            return Ok(ValueType::F64);
                        }
                    }
                }
                // Julia allows unresolved references to remain in compiled code
                // and raises UndefVarError only if execution reaches the load.
                // Macro-expanded code from MacroTools relies on that behavior:
                // rejecting the reference here prevents Julia-valid expansions
                // from compiling. Let load_local emit a generic LoadAny, whose VM
                // path already raises UndefVarError when no local/global/type
                // binding exists (Issue #7556).
                let in_locals = self.locals.contains_key(name.as_str());

                // If this is a const struct that can be inlined, emit NewStruct instead of load
                if !in_locals {
                    if let Some((_struct_name, type_id, field_count)) = self
                        .shared_ctx
                        .global_const_structs
                        .get(name.as_str())
                        .map(|(s, t, f)| (s.clone(), *t, *f))
                    {
                        // Inline the struct constructor: emit NewStruct(type_id, field_count)
                        // For empty structs like `const M = MyType()`, this creates a new instance
                        self.emit(Instr::NewStruct(type_id, field_count));
                        return Ok(ValueType::Struct(type_id));
                    }
                }

                // Bare abstract-numeric params (`x::Real`, `x::Number`, ...) load via
                // `LoadAny` (see `load_local`) because the runtime value keeps its
                // concrete type (Int8/Int64/Float32/...). Their `locals` slot, however,
                // is the annotation's widened `ValueType::F64`/`I64`, so reporting that
                // here would make a direct return `f(x::Real)=x` emit `ReturnF64`, which
                // coerces the concrete runtime value (e.g. `Int64(3)` → `Float64(3.0)`)
                // and the typed caller slot then rejects/mistypes it. Report `Any` to
                // match the `LoadAny` representation, so the direct return uses
                // `ReturnAny` and preserves the concrete runtime type — symmetric with
                // the `infer_julia_type` (#5076/#5169) and `infer_expr_type`
                // (#5167 part 2 / #5243) guards (Issue #5242).
                if self.abstract_numeric_params.contains(name.as_str()) {
                    self.load_local(name)?;
                    return Ok(ValueType::Any);
                }

                // Prefer local type, fall back to global type, then default to Any
                // (not I64, to ensure dynamic dispatch for unknown types)
                let ty = self
                    .locals
                    .get(name.as_str())
                    .cloned()
                    .or_else(|| self.shared_ctx.global_types.get(name.as_str()).cloned())
                    .unwrap_or(ValueType::Any);
                self.load_local(name)?;
                Ok(ty)
            }
            Expr::BinaryOp {
                op, left, right, ..
            } => self.compile_binary_op(op, left, right),
            Expr::UnaryOp { op, operand, span } => self.compile_unary_op(op, operand, *span),
            Expr::Call {
                function,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                ..
            } => {
                if function == "typeof"
                    && args.len() == 1
                    && kwargs.is_empty()
                    && !splat_mask.iter().any(|&is_splat| is_splat)
                    && !kwargs_splat_mask.iter().any(|&is_splat| is_splat)
                {
                    if let Expr::Literal(lit, _) = &args[0] {
                        if let Some(type_name) = literal_typeof_datatype_name(lit) {
                            self.emit(Instr::PushDataType(type_name.to_string()));
                            return Ok(ValueType::DataType);
                        }
                    }
                }
                self.compile_call(function, args, kwargs, splat_mask, kwargs_splat_mask)
            }
            // Structural explicit numeric type-constructor call (Issue
            // #9803): produced only by the shared SSA plan builder
            // (`compile::ssa_ir::plan::numeric_convert_target`) for the bare
            // `Int64(x)` / `Float64(x)` shape. Compiles to exactly the same
            // instructions as the `"Int64"`/`"Float64"` arms of
            // `compile_builtin_types` (the equivalent `Expr::Call` path) so
            // stack lowering is unchanged and the existing peephole fusions
            // (`LoadSlotI64ToF64`, `AddF64I64Slots`, ...) still apply.
            Expr::Convert {
                target, operand, ..
            } => {
                self.compile_expr(operand)?;
                let builtin = match target {
                    NumericConvertTarget::Int64 => BuiltinId::Int64,
                    NumericConvertTarget::Float64 => BuiltinId::Float64,
                };
                self.emit(Instr::CallBuiltin(builtin, 1));
                Ok(match target {
                    NumericConvertTarget::Int64 => ValueType::I64,
                    NumericConvertTarget::Float64 => ValueType::F64,
                })
            }
            Expr::Builtin { name, args, .. } => {
                // Base functions are never implicitly shadowed.
                // To extend Base functions, use Base.func(x::T) = ... syntax.
                if matches!(name, BuiltinOp::TypeOf) && args.len() == 1 {
                    if let Expr::Literal(lit, _) = &args[0] {
                        if let Some(type_name) = literal_typeof_datatype_name(lit) {
                            self.emit(Instr::PushDataType(type_name.to_string()));
                            return Ok(ValueType::DataType);
                        }
                    }
                }
                self.compile_builtin(name, args)
            }
            Expr::ArrayLiteral {
                elements, shape, ..
            } => {
                // Infer types of all elements
                let elem_types: Vec<ValueType> = elements
                    .iter()
                    .map(|elem| self.infer_expr_type(elem))
                    .collect();

                // Determine array element type based on element types. Nested
                // array literals need their inner rank to distinguish
                // Vector{T} from Matrix{T}; ValueType::ArrayOf alone cannot
                // carry that information (Issue #6227).
                let nested_ranks = array_literal_element_ranks(elements);
                let array_elem_type = nested_ranks
                    .as_deref()
                    .and_then(|ranks| infer_nested_array_literal_element_type(&elem_types, ranks))
                    .or_else(|| self.tuple_literal_array_element_type(elements))
                    .or_else(|| self.array_literal_element_type_from_julia_types(elements))
                    .unwrap_or_else(|| {
                        infer_array_element_type(
                            &elem_types,
                            |type_id| self.shared_ctx.get_struct_name(type_id),
                            |name| {
                                self.shared_ctx
                                    .struct_table
                                    .get(name)
                                    .map(|info| info.type_id)
                            },
                        )
                        .0
                    });

                match array_elem_type {
                    ArrayElementType::I64 => {
                        // All integer elements: use Memory{Int64} + Array wrapper.
                        self.emit_array_wrapper_memory_start(ArrayElementType::I64, elements.len());
                        for (index, elem) in elements.iter().enumerate() {
                            self.emit(Instr::PushI64((index + 1) as i64));
                            self.compile_expr_as(elem, ValueType::I64)?;
                            self.emit(Instr::MemorySet);
                        }
                        self.emit_array_wrapper_from_memory_on_stack(shape);
                        // Issue #10076: carry the literal's own rank (shape.len())
                        // instead of erasing it to `None`. This is the ValueType
                        // stored into `self.locals` for a variable bound to this
                        // literal (see `Stmt::Assign` in `compile/stmt.rs`), which
                        // is exactly what `compile_similar`'s no-dims branch in
                        // `builtin_array.rs` consults via `infer_expr_type` — see
                        // the matching fix on `infer_expr_type`'s own
                        // `Expr::ArrayLiteral` arm in `infer/mod.rs`.
                        Ok(ValueType::ArrayOf(ArrayElementType::I64, Some(shape.len())))
                    }
                    ArrayElementType::F64 => {
                        // Numeric elements (with at least one float): use Memory{Float64}.
                        self.emit_array_wrapper_memory_start(ArrayElementType::F64, elements.len());
                        for (index, elem) in elements.iter().enumerate() {
                            self.emit(Instr::PushI64((index + 1) as i64));
                            self.compile_expr_as(elem, ValueType::F64)?;
                            self.emit(Instr::MemorySet);
                        }
                        self.emit_array_wrapper_from_memory_on_stack(shape);
                        Ok(ValueType::ArrayOf(ArrayElementType::F64, Some(shape.len())))
                    }
                    ArrayElementType::ComplexF64 => {
                        self.emit_array_wrapper_memory_start(
                            ArrayElementType::ComplexF64,
                            elements.len(),
                        );
                        for (index, (elem, elem_type)) in
                            elements.iter().zip(elem_types.iter()).enumerate()
                        {
                            self.emit(Instr::PushI64((index + 1) as i64));
                            self.compile_complex_array_element(
                                elem,
                                elem_type,
                                ValueType::ComplexF64,
                                "Complex{Float64}",
                            )?;
                            self.emit(Instr::MemorySet);
                        }
                        self.emit_array_wrapper_from_memory_on_stack(shape);
                        Ok(ValueType::ArrayOf(
                            ArrayElementType::ComplexF64,
                            Some(shape.len()),
                        ))
                    }
                    ArrayElementType::ComplexF32 => {
                        self.emit_array_wrapper_memory_start(
                            ArrayElementType::ComplexF32,
                            elements.len(),
                        );
                        for (index, (elem, elem_type)) in
                            elements.iter().zip(elem_types.iter()).enumerate()
                        {
                            self.emit(Instr::PushI64((index + 1) as i64));
                            self.compile_complex_array_element(
                                elem,
                                elem_type,
                                ValueType::ComplexF32,
                                "Complex{Float32}",
                            )?;
                            self.emit(Instr::MemorySet);
                        }
                        self.emit_array_wrapper_from_memory_on_stack(shape);
                        Ok(ValueType::ArrayOf(
                            ArrayElementType::ComplexF32,
                            Some(shape.len()),
                        ))
                    }
                    ArrayElementType::StructOf(type_id) => {
                        // Struct array - check if we need type promotion (e.g., Int -> Rational, Int -> Complex)
                        let struct_name = self.shared_ctx.get_struct_name(type_id);
                        let is_rational = struct_name
                            .as_ref()
                            .map(|n| crate::bytecode::value::is_rational_type_name(n))
                            .unwrap_or(false);
                        let is_complex = struct_name
                            .as_ref()
                            .map(|n| n.starts_with("Complex"))
                            .unwrap_or(false);
                        // Get the target Complex type name for constructor calls
                        let complex_target_name = struct_name.clone().unwrap_or_default();

                        self.emit_array_wrapper_memory_start(
                            ArrayElementType::StructOf(type_id),
                            elements.len(),
                        );
                        for (index, (elem, elem_type)) in
                            elements.iter().zip(elem_types.iter()).enumerate()
                        {
                            self.emit(Instr::PushI64((index + 1) as i64));
                            if is_rational
                                && matches!(
                                    elem_type,
                                    ValueType::I64
                                        | ValueType::I8
                                        | ValueType::I16
                                        | ValueType::I32
                                        | ValueType::I128
                                        | ValueType::U8
                                        | ValueType::U16
                                        | ValueType::U32
                                        | ValueType::U64
                                        | ValueType::U128
                                )
                            {
                                // Promote integer to Rational{Int64}(n, 1)
                                let span = elem.span();
                                let one = Expr::Literal(Literal::Int(1), span);
                                let rational_call = Expr::Call {
                                    function: "Rational{Int64}".to_string().into(),
                                    args: vec![elem.clone(), one],
                                    kwargs: Vec::new(),
                                    splat_mask: vec![],
                                    kwargs_splat_mask: vec![],
                                    span,
                                };
                                self.compile_expr(&rational_call)?;
                            } else if is_complex
                                && matches!(
                                    elem_type,
                                    ValueType::I64
                                        | ValueType::I8
                                        | ValueType::I16
                                        | ValueType::I32
                                        | ValueType::I128
                                        | ValueType::U8
                                        | ValueType::U16
                                        | ValueType::U32
                                        | ValueType::U64
                                        | ValueType::U128
                                        | ValueType::F64
                                        | ValueType::F32
                                        | ValueType::F16
                                        | ValueType::Bool
                                )
                            {
                                // Promote numeric to Complex{T}(n, 0)
                                let span = elem.span();
                                let zero = Expr::Literal(Literal::Int(0), span);
                                let complex_call = Expr::Call {
                                    function: complex_target_name.clone().into(),
                                    args: vec![elem.clone(), zero],
                                    kwargs: Vec::new(),
                                    splat_mask: vec![],
                                    kwargs_splat_mask: vec![],
                                    span,
                                };
                                self.compile_expr(&complex_call)?;
                            } else if is_complex
                                && matches!(elem_type, ValueType::Struct(_))
                                && *elem_type != ValueType::Struct(type_id)
                            {
                                // Promote a different Complex type to target Complex type
                                // e.g., Complex{Bool} -> Complex{Int64}
                                // Use Complex{T}(real(z), imag(z)) since struct constructors require 2 args
                                let span = elem.span();
                                let real_call = Expr::Call {
                                    function: "real".to_string().into(),
                                    args: vec![elem.clone()],
                                    kwargs: Vec::new(),
                                    splat_mask: vec![],
                                    kwargs_splat_mask: vec![],
                                    span,
                                };
                                let imag_call = Expr::Call {
                                    function: "imag".to_string().into(),
                                    args: vec![elem.clone()],
                                    kwargs: Vec::new(),
                                    splat_mask: vec![],
                                    kwargs_splat_mask: vec![],
                                    span,
                                };
                                let complex_call = Expr::Call {
                                    function: complex_target_name.clone().into(),
                                    args: vec![real_call, imag_call],
                                    kwargs: Vec::new(),
                                    splat_mask: vec![],
                                    kwargs_splat_mask: vec![],
                                    span,
                                };
                                self.compile_expr(&complex_call)?;
                            } else {
                                self.compile_expr(elem)?;
                            }
                            self.emit(Instr::MemorySet);
                        }
                        self.emit_array_wrapper_from_memory_on_stack(shape);
                        Ok(ValueType::ArrayOf(
                            ArrayElementType::StructOf(type_id),
                            Some(shape.len()),
                        ))
                    }
                    ArrayElementType::Bool => {
                        // All boolean elements: use Memory{Bool}.
                        self.emit_array_wrapper_memory_start(
                            ArrayElementType::Bool,
                            elements.len(),
                        );
                        for (index, elem) in elements.iter().enumerate() {
                            self.emit(Instr::PushI64((index + 1) as i64));
                            self.compile_expr_as(elem, ValueType::Bool)?;
                            self.emit(Instr::MemorySet);
                        }
                        self.emit_array_wrapper_from_memory_on_stack(shape);
                        Ok(ValueType::ArrayOf(
                            ArrayElementType::Bool,
                            Some(shape.len()),
                        ))
                    }
                    ArrayElementType::String => {
                        self.emit_array_wrapper_memory_start(
                            ArrayElementType::String,
                            elements.len(),
                        );
                        for (index, elem) in elements.iter().enumerate() {
                            self.emit(Instr::PushI64((index + 1) as i64));
                            self.compile_expr(elem)?;
                            self.emit(Instr::MemorySet);
                        }
                        self.emit_array_wrapper_from_memory_on_stack(shape);
                        Ok(ValueType::ArrayOf(
                            ArrayElementType::String,
                            Some(shape.len()),
                        ))
                    }
                    ArrayElementType::Char => {
                        self.emit_array_wrapper_memory_start(
                            ArrayElementType::Char,
                            elements.len(),
                        );
                        for (index, elem) in elements.iter().enumerate() {
                            self.emit(Instr::PushI64((index + 1) as i64));
                            self.compile_expr(elem)?;
                            self.emit(Instr::MemorySet);
                        }
                        self.emit_array_wrapper_from_memory_on_stack(shape);
                        Ok(ValueType::ArrayOf(
                            ArrayElementType::Char,
                            Some(shape.len()),
                        ))
                    }
                    ArrayElementType::Symbol => {
                        self.emit_array_wrapper_memory_start(
                            ArrayElementType::Symbol,
                            elements.len(),
                        );
                        for (index, elem) in elements.iter().enumerate() {
                            self.emit(Instr::PushI64((index + 1) as i64));
                            self.compile_expr(elem)?;
                            self.emit(Instr::MemorySet);
                        }
                        self.emit_array_wrapper_from_memory_on_stack(shape);
                        Ok(ValueType::ArrayOf(
                            ArrayElementType::Symbol,
                            Some(shape.len()),
                        ))
                    }
                    other => {
                        // Heterogeneous array. Issue #3549: when the inferred
                        // element type is `UnionOf(...)`, propagate it to the
                        // VM so `typeof(a)` reports `Vector{Union{...}}` rather
                        // than `Vector{Any}`. Otherwise fall back to Any.
                        let storage_elem = match &other {
                            ArrayElementType::F16
                            | ArrayElementType::F32
                            | ArrayElementType::I8
                            | ArrayElementType::I16
                            | ArrayElementType::I32
                            | ArrayElementType::I128
                            | ArrayElementType::U8
                            | ArrayElementType::U16
                            | ArrayElementType::U32
                            | ArrayElementType::U64
                            | ArrayElementType::U128
                            | ArrayElementType::Nothing
                            | ArrayElementType::Symbol
                            | ArrayElementType::SubString
                            | ArrayElementType::UnionOf(_)
                            | ArrayElementType::Abstract(_)
                            | ArrayElementType::Structured(_)
                            | ArrayElementType::TupleOf(_) => other.clone(),
                            _ => ArrayElementType::Any,
                        };
                        self.emit_array_wrapper_memory_start(storage_elem, elements.len());
                        for (index, elem) in elements.iter().enumerate() {
                            self.emit(Instr::PushI64((index + 1) as i64));
                            self.compile_expr(elem)?;
                            self.emit(Instr::MemorySet);
                        }
                        self.emit_array_wrapper_from_memory_on_stack(shape);
                        // Issue #10076: carry the literal's rank for concrete
                        // element types, same as the other arms above — but
                        // NOT for a bare `Any` element (heterogeneous/empty
                        // literal). `ArrayOf(Any, Some(n))` is also how
                        // `Expr::Comprehension` represents "rank known,
                        // element type unresolved" (Issue #6817), and the
                        // `infer_julia_type` dispatch bridge treats that
                        // combination as ambiguous, reporting the bare
                        // `Vector`/`Matrix` alias rather than the concrete
                        // `Vector{Any}`/`Matrix{Any}`. For a literal the
                        // `Any` element is exact, not unresolved, so
                        // widening rank here would make an exact
                        // `::Vector{Any}` method parameter statically
                        // un-bindable — see the matching note and the
                        // regression it caused in `infer_expr_type`'s
                        // `Expr::ArrayLiteral` arm (`infer/mod.rs`); tracked
                        // separately as Issue #10206.
                        let rank = if matches!(other, ArrayElementType::Any) {
                            None
                        } else {
                            Some(shape.len())
                        };
                        Ok(ValueType::ArrayOf(other, rank))
                    }
                }
            }
            Expr::TypedEmptyArray { element_type, span } => {
                // Create empty typed array based on element type string
                // Issue #3548: thread the declared element type all the way through
                // so typeof(Int32[]) reports Vector{Int32}, not Vector{Int64}.
                let elem_type = match element_type.as_str() {
                    "Int" if crate::types::native_int_type_name() == "Int32" => {
                        ArrayElementType::I32
                    }
                    "Int" | "Int64" => ArrayElementType::I64,
                    "Int32" => ArrayElementType::I32,
                    "Int16" => ArrayElementType::I16,
                    "Int8" => ArrayElementType::I8,
                    // Issue #3557: Int128/UInt128 use boxed Any storage with
                    // an element-type override so `typeof(Int128[]) ===
                    // Vector{Int128}`.
                    "Int128" => ArrayElementType::I128,
                    "UInt128" => ArrayElementType::U128,
                    "UInt" if crate::types::native_uint_type_name() == "UInt32" => {
                        ArrayElementType::U32
                    }
                    "UInt" | "UInt64" => ArrayElementType::U64,
                    "UInt32" => ArrayElementType::U32,
                    "UInt16" => ArrayElementType::U16,
                    "UInt8" => ArrayElementType::U8,
                    "Float64" => ArrayElementType::F64,
                    "Float32" => ArrayElementType::F32,
                    // Issue #9301: empty `Float16[]` keeps its concrete eltype so
                    // `typeof(Float16[]) === Vector{Float16}` (boxed storage tag,
                    // like Int128/UInt128 above), matching F32/F64.
                    "Float16" => ArrayElementType::F16,
                    "Number" => ArrayElementType::Abstract("Number".to_string()),
                    "Real" => ArrayElementType::Abstract("Real".to_string()),
                    "Integer" => ArrayElementType::Abstract("Integer".to_string()),
                    "Signed" => ArrayElementType::Abstract("Signed".to_string()),
                    "Unsigned" => ArrayElementType::Abstract("Unsigned".to_string()),
                    "AbstractFloat" => ArrayElementType::Abstract("AbstractFloat".to_string()),
                    "Complex{Float64}" | "ComplexF64" => ArrayElementType::ComplexF64,
                    "Complex{Float32}" | "ComplexF32" => ArrayElementType::ComplexF32,
                    "Union{}" => ArrayElementType::UnionOf(Vec::new()),
                    "Bool" => ArrayElementType::Bool,
                    "String" => ArrayElementType::String,
                    "Char" => ArrayElementType::Char,
                    // Issue #5711: an empty `Symbol[]` / `Regex[]` literal must keep its
                    // declared element type so `eltype` / `typeof` match upstream (the
                    // catch-all below would otherwise widen them to `Any`). `Symbol` has
                    // a dedicated storage tag; `Regex` / `RegexMatch` are boxed scalar
                    // values stored in an `Abstract`-tagged slot (mirrors the non-empty
                    // `Regex[...]` literal, Issue #5706).
                    "Symbol" => ArrayElementType::Symbol,
                    "Regex" => ArrayElementType::Abstract("Regex".to_string()),
                    "RegexMatch" => ArrayElementType::Abstract("RegexMatch".to_string()),
                    "Any" => ArrayElementType::Any,
                    type_name => {
                        // Check if it's a struct type (Complex{Float64}, Point{Int}, etc.)
                        // Extract base name before {
                        let base_name = type_name.split('{').next().unwrap_or(type_name);

                        if let Some(elem_type) =
                            concrete_parametric_element_type_from_name(type_name)
                        {
                            // Parametric structs such as `UnitRange{Int64}[]`
                            // must keep the full instantiated eltype. Once
                            // UnitRange became a real Base struct, resolving
                            // the bare family first collapsed this to
                            // `UnitRange{Any}`.
                            elem_type
                        } else if let Some(type_id) = self.shared_ctx.get_struct_type_id(base_name)
                        {
                            // Look up non-parametric struct types in the shared context.
                            ArrayElementType::StructOf(type_id)
                        } else if self.locals.contains_key(type_name)
                            || self.shared_ctx.global_types.contains_key(type_name)
                            || self.shared_ctx.global_const_structs.contains_key(type_name)
                            || self.captured_vars.contains(type_name)
                        {
                            // Issue #6839: `name[]` where `name` is a VALUE binding
                            // (a `const` global, local, captured var, or a variable
                            // bound to a type — e.g. `const LOG = Ref(0); LOG[]`, or
                            // `T = Int; T[]`) is `getindex(name)`, NOT the typed
                            // empty-array literal `T[]`. Only genuine type *names*
                            // build an empty `Vector{T}`; recognized builtin types and
                            // user structs are claimed by the arms above and the
                            // `get_struct_type_id` branch, so a value binding only ever
                            // reaches this fallback. Routing to `getindex` lets
                            // dispatch pick the right method — `getindex(::Ref)` reads
                            // the ref, `getindex(::Type{T})` builds the empty vector.
                            let var = Expr::Var(type_name.to_string().into(), *span);
                            return self.compile_call("getindex", &[var], &[], &[], &[]);
                        } else if type_name
                            .chars()
                            .all(|c| c.is_alphanumeric() || c == '_' || c == '!')
                        {
                            // Issue #10583: a bare-identifier head the compiler
                            // cannot resolve to any type or value binding must
                            // NOT silently become `Any[]`. Upstream lowers
                            // `T[]` to `getindex(T)` after resolving `T` as an
                            // ordinary global — an undefined head raises
                            // `UndefVarError` at runtime, and a runtime-only
                            // binding still reaches the right `getindex`
                            // method. Compound heads (`Foo{Int}`, `Union{...}`,
                            // dotted paths) keep the permissive fallback below.
                            let var = Expr::Var(type_name.to_string().into(), *span);
                            return self.compile_call("getindex", &[var], &[], &[], &[]);
                        } else {
                            ArrayElementType::Any
                        }
                    }
                };

                // Emit an empty Memory-backed Array wrapper (Issue #6649).
                self.emit_empty_array_wrapper(elem_type.clone(), &[0]);

                Ok(ValueType::ArrayOf(elem_type, None))
            }
            Expr::Index {
                array,
                indices,
                span,
            } => {
                // `d[k1, k2, ...]` on an AbstractDict is sugar for `d[(k1, k2, ...)]`:
                // upstream defines `getindex(t::AbstractDict, k1, k2, ks...) =
                // getindex(t, tuple(k1, k2, ks...))` (abstractdict.jl). Without this,
                // a Dict receiver with 2+ plain indices falls through to native
                // multi-dim array indexing (`IndexLoad(N)`), which errors on a Dict
                // (Issue #6707). Rewrite to a single tuple key and dispatch the
                // ordinary one-key `getindex`. Slice indices are left alone (a Dict
                // has no slice indexing; let the normal path report the error).
                if indices.len() >= 2
                    && !indices
                        .iter()
                        .any(|idx| matches!(idx, Expr::Range { .. } | Expr::SliceAll { .. }))
                {
                    let receiver_julia = self.infer_julia_type(array);
                    let receiver_is_dict_like = matches!(
                        self.infer_expr_type(array),
                        ValueType::Dict
                    ) || matches!(receiver_julia, JuliaType::Dict)
                        || matches!(&receiver_julia, JuliaType::Struct(name) if is_dict_struct_name(name))
                        || matches!(self.infer_expr_type(array), ValueType::Struct(type_id)
                            if self
                                .shared_ctx
                                .type_id_to_struct_name
                                .get(&type_id)
                                .is_some_and(|name| is_dict_struct_name(name)));
                    if receiver_is_dict_like {
                        let key = Expr::TupleLiteral {
                            elements: indices.clone(),
                            span: *span,
                        };
                        let new_args = vec![array.as_ref().clone(), key];
                        return self.compile_call("getindex", &new_args, &[], &[], &[]);
                    }
                }

                // Julia-compliant: s[i] is equivalent to getindex(s, i)
                // Build arguments for getindex call: [collection, indices...]
                let mut getindex_args = vec![array.as_ref().clone()];
                getindex_args.extend(indices.clone());
                let getindex_arg_types: Vec<JuliaType> = getindex_args
                    .iter()
                    .map(|arg| self.infer_julia_type(arg))
                    .collect();
                // Opaque `ValueType::Dict` receivers must dispatch as Dicts too
                // (Issue #8397): `Dict(x => v)` can widen when `x` comes from a
                // macro/global package value such as `Symbolics.Num`, and falling
                // through to `IndexLoad` treats that non-integer key as an array
                // index.
                let receiver_is_dict_like = match self.infer_expr_type(array) {
                    ValueType::Dict => true,
                    ValueType::Struct(type_id) => self
                        .shared_ctx
                        .type_id_to_struct_name
                        .get(&type_id)
                        .is_some_and(|name| is_dict_struct_name(name)),
                    _ => {
                        matches!(self.infer_julia_type(array), JuliaType::Dict)
                            || matches!(
                                self.infer_julia_type(array),
                                JuliaType::Struct(ref name) if is_dict_struct_name(name)
                            )
                    }
                };
                if receiver_is_dict_like {
                    return self.compile_call("getindex", &getindex_args, &[], &[], &[]);
                }
                let has_slice_like_index = indices.iter().any(|idx| {
                    if matches!(idx, Expr::Range { .. } | Expr::SliceAll { .. }) {
                        return true;
                    }
                    let idx_type = self.infer_expr_type(idx);
                    let idx_julia_type = self.infer_julia_type(idx);
                    is_julia_array_like_type(&idx_julia_type)
                        || self.is_array_wrapper_value_type(&idx_type)
                        || matches!(
                            idx_type,
                            ValueType::Array
                                | ValueType::ArrayOf(_, _)
                                | ValueType::Bool
                                | ValueType::Range
                                | ValueType::Rng
                        )
                });
                if has_slice_like_index
                    && getindex_arg_types
                        .first()
                        .is_some_and(is_julia_array_like_type)
                {
                    return self.compile_builtin_call("getindex", &getindex_args);
                }
                if self.typed_array_literal_element_type(array).is_some() {
                    // `Pair{Int,Int}[...]` and other typed literals must be
                    // materialized by the literal builder before generic
                    // `getindex(::Type, ...)` dispatch can claim the call
                    // (Issue #5233).
                    return self.compile_builtin_call("getindex", &getindex_args);
                }
                if self.has_user_dispatch_method_for_arg_types(
                    &["getindex", "Base.getindex"],
                    &getindex_arg_types,
                ) {
                    return self.compile_call("getindex", &getindex_args, &[], &[], &[]);
                }
                // Issue #6657: an `Any`-typed receiver cannot match a concrete
                // user `getindex` override at compile time, so the check above
                // is false even when the runtime value would dispatch to a user
                // method (e.g. `f(xs) = xs[1]` called with a `Vector` that has a
                // user override). Route it through a runtime dispatch with a
                // native-indexing fallback before the builtin fast path.
                if let Some(result) = self.try_compile_dynamic_getindex_dispatch(&getindex_args) {
                    return result;
                }

                // Special case: typed arrays need IndexLoadTyped for proper type preservation
                let is_typed_array = if let Expr::Var(name, _) = array.as_ref() {
                    matches!(
                        self.locals.get(name.as_str()),
                        Some(ValueType::ArrayOf(_, _))
                    )
                } else {
                    false
                };

                if is_typed_array {
                    // Check for slice-like indices: Range, SliceAll, Array, or Range variable (Issue #3481)
                    let has_slice = indices.iter().any(|idx| {
                        match idx {
                            Expr::Range { .. } | Expr::SliceAll { .. } => true,
                            _ => {
                                // Array index could be logical indexing (bool array), index array,
                                // or a Range variable
                                let idx_type = self.infer_expr_type(idx);
                                let idx_julia_type = self.infer_julia_type(idx);
                                is_julia_array_like_type(&idx_julia_type)
                                    || self.is_array_wrapper_value_type(&idx_type)
                                    || is_range_like_julia_type(&idx_julia_type)
                                    || matches!(
                                        idx_type,
                                        ValueType::Array
                                            | ValueType::ArrayOf(_, _)
                                            | ValueType::Bool
                                            | ValueType::Range
                                            | ValueType::Rng
                                    )
                            }
                        }
                    });

                    // Get return type for typed arrays
                    let has_dynamic_index = indices.iter().any(|idx| {
                        matches!(
                            self.infer_expr_type(idx),
                            ValueType::Any | ValueType::Struct(_)
                        )
                    });
                    let return_type = if has_dynamic_index {
                        None
                    } else if let Expr::Var(name, _) = array.as_ref() {
                        if let Some(ValueType::ArrayOf(elem_type, _)) =
                            self.locals.get(name.as_str())
                        {
                            match elem_type {
                                ArrayElementType::StructOf(type_id) => {
                                    Some(ValueType::Struct(*type_id))
                                }
                                ArrayElementType::I64 => Some(ValueType::I64),
                                ArrayElementType::F64 => Some(ValueType::F64),
                                _ => None,
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    self.compile_expr(array)?;
                    for idx in indices {
                        match idx {
                            Expr::Range { .. } | Expr::SliceAll { .. } => {
                                self.compile_expr(idx)?;
                            }
                            _ => {
                                // Check if index might be a CartesianIndex (struct type), Array,
                                // or Range variable (Issue #3481) — compile as-is, no I64 coercion
                                let idx_type = self.infer_expr_type(idx);
                                if matches!(
                                    idx_type,
                                    ValueType::Struct(_)
                                        | ValueType::Any
                                        | ValueType::Array
                                        | ValueType::ArrayOf(_, _)
                                        | ValueType::Bool
                                        | ValueType::Range
                                        | ValueType::Rng
                                ) {
                                    self.compile_expr(idx)?;
                                } else {
                                    self.compile_expr_as(idx, ValueType::I64)?;
                                }
                            }
                        }
                    }
                    if has_slice {
                        self.emit(Instr::IndexSlice(indices.len()));
                        Ok(ValueType::Array)
                    } else if indices.len() == 1
                        && (self.inbounds_context
                            || self.is_proven_inbounds_index(array.as_ref(), &indices[0]))
                    {
                        self.emit(Instr::IndexLoadTypedInbounds(indices.len()));
                        Ok(return_type.unwrap_or(ValueType::Any))
                    } else {
                        self.emit(Instr::IndexLoadTyped(indices.len()));
                        Ok(return_type.unwrap_or(ValueType::Any))
                    }
                } else {
                    // Use getindex builtin for all other types (Dict, Tuple, String, Array)
                    self.compile_builtin_call("getindex", &getindex_args)
                }
            }
            Expr::Range {
                start,
                step,
                stop,
                span,
            } => {
                let inferred_range_ty = self.infer_structural_range_julia_type(
                    start,
                    step.as_ref().map(AsRef::as_ref),
                    stop,
                );
                if let Some(constructor) = concrete_range_constructor_name(&inferred_range_ty) {
                    let mut args = vec![start.as_ref().clone()];
                    if let Some(step_expr) = step {
                        args.push(step_expr.as_ref().clone());
                    } else if constructor == "StepRange" {
                        args.push(Expr::Literal(Literal::Int(1), *span));
                    }
                    args.push(stop.as_ref().clone());
                    let arg_types = args
                        .iter()
                        .map(|arg| self.infer_julia_type(arg))
                        .collect::<Vec<_>>();
                    let base_constructor_wins =
                        self.base_owned_dispatch_wins(constructor, &arg_types);
                    // Unit-range colon syntax is Base-owned: upstream lowers
                    // `a:b` through `Base.:(:)` straight into the parametric
                    // `UnitRange{T}(start, stop)` inner constructor, so a user
                    // outer constructor on the bare `UnitRange` name never
                    // participates (Issue #11444). When the bare table would
                    // dispatch to a non-Base method, compile the fully applied
                    // parametric spelling instead of the hijackable bare name.
                    // `a:s:b` is deliberately NOT rerouted: upstream's
                    // `_colon` calls the BARE `StepRange(start, step, stop)`,
                    // so an imported user extension legitimately intercepts
                    // step-range literals (pinned by
                    // `range_type_recovery_respects_user_constructor_return_11434`).
                    let call_name = match &inferred_range_ty {
                        JuliaType::Struct(name)
                            if constructor == "UnitRange"
                                && !base_constructor_wins
                                && name.contains('{') =>
                        {
                            name.clone()
                        }
                        _ => constructor.to_string(),
                    };
                    let compiled = self.compile_expr(&Expr::Call {
                        function: call_name.into(),
                        args,
                        kwargs: Vec::new(),
                        splat_mask: Vec::new(),
                        kwargs_splat_mask: Vec::new(),
                        span: *span,
                    })?;
                    if matches!(compiled, ValueType::Any) && base_constructor_wins {
                        if let JuliaType::Struct(name) = &inferred_range_ty {
                            if let Some((_, type_args)) = parse_parametric_call(name) {
                                if let Ok(type_id) =
                                    self.shared_ctx.resolve_instantiation_with_type_expr(
                                        &format!("Base.{constructor}"),
                                        &type_args,
                                    )
                                {
                                    return Ok(ValueType::Struct(type_id));
                                }
                            }
                        }
                    }
                    return Ok(compiled);
                }

                // Create lazy Range value (does not materialize to array).
                // MakeRangeLazy/MakeStepRangeLazy expect: start, step, stop on stack.
                // An explicit step (`a:s:b`) makes a `StepRange` even if the step is 1
                // (`1:1:5`), distinguished from the `UnitRange` `1:5` (Issue #5667).
                let explicit_step = step.is_some();
                self.compile_expr(start)?;
                if let Some(s) = step {
                    self.compile_expr(s)?;
                } else {
                    self.emit(Instr::PushI64(1));
                }
                self.compile_expr(stop)?;
                self.emit(if explicit_step {
                    Instr::MakeStepRangeLazy
                } else {
                    Instr::MakeRangeLazy
                });
                Ok(ValueType::Range)
            }
            Expr::Comprehension {
                body,
                var,
                iter,
                filter,
                ..
            } => self.compile_comprehension(body, var, iter, filter.as_deref()),
            Expr::MultiComprehension {
                body,
                iterations,
                filter,
                flatten,
                ..
            } => self.compile_multi_comprehension(body, iterations, filter.as_deref(), *flatten),
            Expr::Generator {
                body,
                var,
                iter,
                filter,
                span,
            } => self.compile_generator_expr(body, var, iter, filter.as_deref(), *span),
            Expr::FieldAccess { object, field, .. } => self.compile_field_access(object, field),
            Expr::SliceAll { .. } => {
                self.emit(Instr::SliceAll);
                Ok(ValueType::Array)
            }
            Expr::FunctionRef { name, span } => {
                let _ = span;
                // Check if this function reference is a closure that captures variables
                // from the outer scope (Issue #2358)
                //
                // Lambda functions defined at module level (e.g., in @testset blocks)
                // have their captured variables pre-analyzed during main block setup.
                if let Some((qualified_name, captures)) = self
                    .scoped_closure_captures(name.as_str())
                    .map(|(qualified_name, captures)| (qualified_name.clone(), captures.clone()))
                {
                    if !captures.is_empty() {
                        // This is a closure - emit CreateClosure instead of PushFunction
                        let mut capture_names: Vec<String> = captures.iter().cloned().collect();
                        // `closure_captures` is a HashSet. Canonicalize its
                        // process-random iteration order before it reaches
                        // serialized bytecode (Issue #11264).
                        capture_names.sort();
                        let mut candidate_indices =
                            self.lowering_helper_function_ref_candidates(expr);
                        if candidate_indices.is_empty() {
                            candidate_indices =
                                self.imported_generic_candidate_indices(&qualified_name);
                        }
                        self.emit_closure_value(&qualified_name, capture_names, candidate_indices);
                        return Ok(ValueType::Any);
                    }
                }
                // Marker-less lowering helpers are private callables, not
                // members of a Julia-visible generic function that happens to
                // share their spelling. Their source span identifies the exact
                // helper family, so freeze those indices directly instead of
                // consulting the public method table (Issue #9784).
                let helper_candidates = self.lowering_helper_function_ref_candidates(expr);
                if !helper_candidates.is_empty() {
                    self.emit(Instr::PushResolvedFunction(Box::new(
                        crate::bytecode::ResolvedFunctionOperands {
                            name: name.to_string(),
                            candidate_indices: helper_candidates,
                        },
                    )));
                    return Ok(ValueType::Function);
                }
                // Regular function reference (not a closure)
                self.emit_function_value(name);
                Ok(ValueType::Function)
            }
            Expr::TupleLiteral { elements, .. } => {
                // Compile each element and create tuple
                for elem in elements {
                    self.compile_expr(elem)?;
                }
                self.emit(Instr::NewTuple(elements.len()));
                Ok(ValueType::Tuple)
            }
            Expr::NamedTupleLiteral { fields, .. } => {
                // Compile each field value and create named tuple
                let names: Vec<String> = fields.iter().map(|(name, _)| name.to_string()).collect();
                for (_, value) in fields {
                    self.compile_expr(value)?;
                }
                self.emit(Instr::NewNamedTuple(names));
                Ok(ValueType::NamedTuple)
            }
            Expr::Pair { key, value, .. } => {
                // Issue #4346: `a => b` is a Pair, not a Tuple. Emitting a
                // Tuple lets Pair-specific methods receive the wrong runtime
                // representation after dispatch.
                if let Some(struct_info) = self.shared_ctx.struct_table.get("Pair").cloned() {
                    let args = vec![key.as_ref().clone(), value.as_ref().clone()];
                    self.compile_struct_constructor(struct_info, &args)
                } else {
                    self.compile_expr(key)?;
                    self.compile_expr(value)?;
                    self.emit(Instr::NewTuple(2));
                    Ok(ValueType::Tuple)
                }
            }
            Expr::DictLiteral { pairs, span } => {
                let args: Vec<Expr> = pairs
                    .iter()
                    .map(|(key, value)| Expr::Pair {
                        key: Box::new(key.clone()),
                        value: Box::new(value.clone()),
                        span: *span,
                    })
                    .collect();
                self.compile_call("Dict", &args, &[], &[], &[])
            }
            Expr::LetBlock {
                bindings,
                body,
                span,
            } => {
                let opens_testset_scope = block_opens_testset_scope(body);
                if bindings.is_empty() && !opens_testset_scope {
                    // Empty-binding LetBlocks are used for begin/block expressions
                    // in value position. They do not introduce a Julia local scope,
                    // so assignments inside must remain visible afterward. Preserve
                    // every binding owned by this transparent body while compiling
                    // nested soft scopes. Lexical membership is distinct from
                    // initialization/liveness; module-mode stores still route by
                    // `local_scope_depth == 0` and remain globals.
                    let previous_scope = self.lexical_scope_locals.clone();
                    crate::lowering::soft_scope::collect_scope_level_bindings(
                        body,
                        &mut self.lexical_scope_locals,
                    );
                    let result = self.compile_block_value(body);
                    self.lexical_scope_locals = previous_scope;
                    return result;
                }

                if self.explicit_lexical_scopes {
                    return self.compile_explicit_let_block(bindings, body, opens_testset_scope);
                }

                // Let blocks introduce local bindings and evaluate the body
                // Track which bindings shadow existing variables so we can restore them
                //
                // FIX for Issue #1361: Store old values in temporary variables instead of
                // on the stack. Using the stack with Swap operations is unsafe when the
                // body contains nested function calls that modify the stack.
                let let_outer_locals = self.locals.clone();
                let let_outer_initialized_locals = self.initialized_locals.clone();
                let let_outer_julia_type_locals = self.julia_type_locals.clone();
                let let_outer_known_any_rank_array_locals =
                    self.known_any_rank_array_locals.clone();
                let let_outer_mixed_type_vars = self.mixed_type_vars.clone();
                let let_outer_declared_globals = self.declared_globals.clone();
                let let_outer_lexical_scope_locals = self.lexical_scope_locals.clone();
                let mut shadowed: Vec<(String, ValueType, String)> = Vec::new();
                let mut introduced: Vec<String> = Vec::new();
                let mut let_declared_globals = HashSet::new();
                super::stmt::collect_declared_globals(body, &mut let_declared_globals);
                // `collect_declared_globals` does not descend scope-transparent /
                // `@testset` inner blocks, but the let-local collection below does;
                // collect the nested `global` declarations with the matching
                // descent so a `global x` inside a `@testset`/`begin` is excluded
                // from the let-local set and never forgotten (Issue #9313).
                collect_let_body_declared_globals(body, &mut let_declared_globals);
                let mut let_local_names: HashSet<String> =
                    bindings.iter().map(|(name, _)| name.to_string()).collect();
                collect_let_body_assignment_names(body, &mut let_local_names);
                let_local_names.extend(
                    crate::lowering::soft_scope::ScopeBindingInventory::collect(body)
                        .binding_names()
                        .cloned(),
                );
                for name in &let_declared_globals {
                    let_local_names.remove(name);
                }
                let mut let_lexical_scope_locals = self.lexical_scope_locals.clone();
                let_lexical_scope_locals.extend(bindings.iter().map(|(name, _)| name.to_string()));
                crate::lowering::soft_scope::collect_scope_level_bindings(
                    body,
                    &mut let_lexical_scope_locals,
                );
                for name in &let_declared_globals {
                    let_lexical_scope_locals.remove(name);
                }

                // Save old values of variables that will be shadowed to temporary variables
                for var in &let_local_names {
                    let old_ty_opt = self.locals.get(var.as_str()).cloned();
                    if let Some(old_ty) = old_ty_opt {
                        if !self.initialized_locals.contains(var) {
                            continue;
                        }
                        // Generate unique temporary variable name using span info
                        let temp_name = format!("__letblock_shadow_{}_{}", var, span.start);
                        // Load old value and store it to temporary variable
                        self.load_local(var)?;
                        self.emit(Instr::StoreAny(temp_name.clone()));
                        shadowed.push((var.clone(), old_ty, temp_name));
                    } else {
                        introduced.push(var.clone());
                    }
                }

                // Hard-scope `let` discards its body-local bindings at block exit
                // (Issue #9313): every let-local that does NOT shadow an
                // initialized outer binding must be forgotten so an outer
                // `@isdefined`/read after the `let` sees it as undefined. Shadowed
                // names are restored below instead, so record them to exclude them
                // here. Testset `global` names (collected once the body is scanned)
                // are also excluded, since they intentionally persist as module
                // globals.
                let shadowed_names: HashSet<String> =
                    shadowed.iter().map(|(name, _, _)| name.clone()).collect();
                let mut forget_exclude: HashSet<String> = HashSet::new();

                // Store the bindings in locals. Their RHS expressions can contain
                // lowered helper functions (for example generator bodies), and
                // those helpers belong to this hard local scope too.
                let binding_outer_local_scope_depth = self.local_scope_depth;
                self.local_scope_depth += 1;
                let binding_result: CResult<()> = (|| {
                    for (var, value) in bindings {
                        if matches!(value, Expr::Literal(Literal::Undef, _)) {
                            self.locals.insert(var.to_string(), ValueType::Any);
                            self.initialized_locals.remove(var.as_str());
                            continue;
                        }
                        let ty = self.compile_expr(value)?;
                        self.locals.insert(var.to_string(), ty.clone());
                        self.store_local(var, ty);
                    }
                    Ok(())
                })();
                self.local_scope_depth = binding_outer_local_scope_depth;
                binding_result?;

                // Assignment anywhere in a hard `let` makes that name local
                // throughout the whole scope, including reads before the
                // assignment executes. Predeclare body-assigned locals as
                // uninitialized so they shadow imports/modules immediately and
                // a reached read raises UndefVarError (Issue #11176).
                let explicit_binding_names: HashSet<&str> =
                    bindings.iter().map(|(name, _)| name.as_str()).collect();
                for name in &let_local_names {
                    if !explicit_binding_names.contains(name.as_str()) {
                        self.locals.insert(name.clone(), ValueType::Any);
                        self.initialized_locals.remove(name);
                    }
                }

                // Compile all statements in the body. Macro-expanded @testset
                // bodies arrive here as LetBlocks containing _testset_begin! /
                // _testset_end!, and should behave as Julia local scopes.
                let outer_locals = opens_testset_scope.then(|| self.locals.clone());
                let outer_julia_type_locals =
                    opens_testset_scope.then(|| self.julia_type_locals.clone());
                let outer_known_any_rank_array_locals =
                    opens_testset_scope.then(|| self.known_any_rank_array_locals.clone());
                let outer_mixed_type_vars =
                    opens_testset_scope.then(|| self.mixed_type_vars.clone());
                let outer_declared_globals =
                    opens_testset_scope.then(|| self.declared_globals.clone());
                let outer_local_scope_depth = self.local_scope_depth;
                let mut testset_declared_globals = std::collections::HashSet::new();
                self.declared_globals
                    .extend(let_declared_globals.iter().cloned());
                self.lexical_scope_locals = let_lexical_scope_locals;
                self.local_scope_depth += 1;
                if opens_testset_scope {
                    collect_declared_globals_in_testset_scope(body, &mut testset_declared_globals);
                    self.declared_globals
                        .extend(testset_declared_globals.iter().cloned());
                    // A `global x` inside a `@testset` binds the module global and
                    // must survive the block, so never forget it (Issue #9313).
                    forget_exclude.extend(testset_declared_globals.iter().cloned());
                }
                let result_ty = {
                    let stmts = &body.stmts;
                    let result = if stmts.is_empty() {
                        // Empty block returns nothing
                        self.emit(Instr::PushNothing);
                        Ok(ValueType::Nothing)
                    } else {
                        self.compile_block_value(body)
                    };
                    self.local_scope_depth = outer_local_scope_depth;
                    result?
                };
                if opens_testset_scope {
                    if let Some(outer) = outer_locals {
                        self.locals = outer;
                    }
                    if let Some(outer) = outer_julia_type_locals {
                        self.julia_type_locals = outer;
                    }
                    if let Some(outer) = outer_known_any_rank_array_locals {
                        self.known_any_rank_array_locals = outer;
                    }
                    if let Some(outer) = outer_mixed_type_vars {
                        self.mixed_type_vars = outer;
                    }
                    if let Some(outer) = outer_declared_globals {
                        self.declared_globals = outer;
                    }
                    for name in testset_declared_globals {
                        self.locals.insert(name.clone(), ValueType::Any);
                        self.julia_type_locals.remove(&name);
                        self.known_any_rank_array_locals.remove(&name);
                        self.mixed_type_vars.insert(name);
                    }
                }

                for var in introduced {
                    self.locals.remove(&var);
                    self.julia_type_locals.remove(&var);
                    self.known_any_rank_array_locals.remove(&var);
                    self.mixed_type_vars.remove(&var);
                }

                // Restore shadowed variables from temporary storage
                // The result is on top of stack, no need for Swap operations
                for (var, old_ty, temp_name) in shadowed {
                    // Load old value from temporary variable
                    self.emit(Instr::LoadAny(temp_name));
                    // Store it back to the original variable
                    self.store_local(&var, old_ty.clone());
                    self.locals.insert(var, old_ty);
                }
                // Let-local names introduced anywhere in the body must not leak into
                // subsequent branch compilation. Otherwise a later branch can treat
                // them as runtime-shadowed and emit a load for a binding that never
                // existed on that path (Issue #7570).
                if !opens_testset_scope {
                    self.locals = let_outer_locals;
                    self.initialized_locals = let_outer_initialized_locals;
                    self.julia_type_locals = let_outer_julia_type_locals;
                    self.known_any_rank_array_locals = let_outer_known_any_rank_array_locals;
                    self.mixed_type_vars = let_outer_mixed_type_vars;
                    self.declared_globals = let_outer_declared_globals;
                }
                self.lexical_scope_locals = let_outer_lexical_scope_locals;

                // Discard the genuine let-locals from the runtime frame at block
                // exit so they do not leak into the enclosing/module scope
                // (Issue #9313). The let's result value stays on top of the stack;
                // `ForgetLetLocals` has no stack effect.
                //
                // Function inlining reuses a `let` whose bindings are synthetic
                // `__sjulia_inline_arg_*` slots (compile::ir_inline); that is a
                // compiler-generated wrapper, not a user scope, so forgetting its
                // locals is pointless and blocks const-folding of the inlined body
                // (Issue #8443 regression). Skip inline wrappers entirely, and for
                // a real `let` never forget compiler-internal names — the synthetic
                // scope marker, inline temps (`__sjulia*`) or gensym/testset temps
                // (names containing `#`) — which are not user-visible and can
                // interfere with peephole/const-folding.
                let is_inline_wrapper = bindings
                    .iter()
                    .any(|(name, _)| name.starts_with("__sjulia_inline_arg_"));
                if !is_inline_wrapper {
                    let mut to_forget: Vec<String> = let_local_names
                        .iter()
                        .filter(|name| {
                            !shadowed_names.contains(*name)
                                && !forget_exclude.contains(*name)
                                && !name.starts_with("__sjulia")
                                && !name.contains('#')
                        })
                        .cloned()
                        .collect();
                    if !to_forget.is_empty() {
                        // Sort for deterministic bytecode (stable across HashSet order).
                        to_forget.sort();
                        self.emit(Instr::ForgetLetLocals(to_forget));
                    }
                }

                Ok(result_ty)
            }
            Expr::StringConcat { parts, .. } => {
                // Compile each part (they will be pushed on the stack)
                for part in parts {
                    self.compile_expr(part)?;
                }
                // Emit StringConcat instruction to concatenate all parts
                self.emit(Instr::StringConcat(parts.len()));
                Ok(ValueType::Str)
            }
            Expr::ModuleCall {
                module,
                function,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                ..
            } => self.compile_module_call(
                module,
                function,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
            ),
            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => {
                let condition_value = {
                    let current_type_for = |name: &str| self.locals.get(name).cloned();
                    super::narrowing::const_nothing_guard_bool(condition, &current_type_for)
                };
                if let Some(condition_value) = condition_value {
                    return if condition_value {
                        self.compile_expr(then_expr)
                    } else {
                        self.compile_expr(else_expr)
                    };
                }

                // Compile: condition ? then_expr : else_expr
                // Similar to if-else but as an expression. Branch-context
                // lowering avoids materializing `&&` / `||` as stack Bools
                // before the conditional jump.
                let condition_false_jumps = self.compile_condition_false_jumps(condition)?;
                let then_restore = self.apply_then_narrowings(condition);
                let then_type = self.compile_expr(then_expr)?;
                self.restore_then_narrowings(then_restore);
                let j_end = self.here();
                self.emit(Instr::Jump(usize::MAX)); // Placeholder

                let else_start = self.here();
                for patch_pos in condition_false_jumps {
                    self.patch_jump(patch_pos, else_start);
                }
                let else_restore = self.apply_else_narrowings(condition);
                let else_type = self.compile_expr(else_expr)?;
                self.restore_then_narrowings(else_restore);

                let end = self.here();
                self.patch_jump(j_end, end);
                // Return the unified type (prefer Any if types differ)
                if then_type == else_type {
                    Ok(then_type)
                } else {
                    Ok(ValueType::Any)
                }
            }
            Expr::New {
                type_args,
                args,
                is_splat,
                span,
            } => {
                // `new(args...)` - create a new instance of the enclosing struct
                // For parametric structs, use dynamic struct creation with type bindings
                if let Some(base_name) = self.current_parametric_struct_name.clone() {
                    // Parametric struct: emit NewParametricStruct which resolves type at runtime
                    if *is_splat {
                        return Err(CompileError::Msg(
                            "new(args...) with splat not yet supported for parametric structs"
                                .to_string(),
                        ));
                    }
                    // Explicit `new{A,B}(...)`: when every spelled-out type
                    // parameter resolves to a concrete value at compile time
                    // (either a literal concrete type or a `where`-clause type
                    // variable that is bound from an argument), materialize them
                    // in source order so the instantiation is named & ordered
                    // correctly (e.g. `Swap{Int64, Float64}` instead of dropping
                    // `Float64`). Otherwise fall back to the runtime
                    // type-binding-driven `NewParametricStruct` so we never crash
                    // on an as-yet-unbound parameter (explicit instantiation such
                    // as `Foo{Float64}(1)` still needs call-site type-arg
                    // plumbing — see Issue #5059). (Issue #5059)
                    if !type_args.is_empty()
                        && type_args.iter().all(|ty| self.type_expr_is_resolvable(ty))
                    {
                        for arg in args {
                            self.compile_expr(arg)?;
                        }
                        for ty in type_args {
                            self.compile_type_expr_as_value(ty)?;
                        }
                        self.emit(Instr::NewDynamicParametricStruct(
                            base_name,
                            args.len(),
                            type_args.len(),
                        ));
                        return Ok(ValueType::Any); // Type determined at runtime
                    }
                    for arg in args {
                        self.compile_expr(arg)?;
                    }
                    self.emit(Instr::NewParametricStruct(base_name, args.len()));
                    return Ok(ValueType::Any); // Type determined at runtime
                }
                if let Some(type_id) = self.current_struct_type_id {
                    if *is_splat {
                        // new(args...) - splat a tuple/array into struct fields
                        if args.len() != 1 {
                            return Err(CompileError::Msg(
                                "new(args...) requires exactly one splat argument".to_string(),
                            ));
                        }
                        self.compile_expr(&args[0])?;
                        self.emit(Instr::NewStructSplat(type_id));
                    } else {
                        for arg in args {
                            self.compile_expr(arg)?;
                        }
                        self.emit(Instr::NewStruct(type_id, args.len()));
                    }
                    Ok(ValueType::Struct(type_id))
                } else {
                    // Outside a lexically-authorized inner constructor, `new`
                    // is an ordinary name lookup. In particular, a function
                    // introduced through runtime `@eval` must not inherit the
                    // enclosing struct helper's privileged constructor owner:
                    // upstream raises a catchable UndefVarError (or calls a
                    // user binding named `new`) instead of rejecting the whole
                    // program during compilation (Issue #11197).
                    let callable = format!("__ownerless_new_callable_{}", span.start);
                    let mut callable_type = self.compile_expr(&Expr::Var("new".into(), *span))?;
                    if !type_args.is_empty() {
                        for type_arg in type_args {
                            self.compile_type_expr_as_value(type_arg)?;
                        }
                        self.emit(Instr::ApplyTypeDynamic(type_args.len()));
                        callable_type = ValueType::DataType;
                    }
                    self.store_local(&callable, callable_type);
                    let ordinary_call = Expr::Call {
                        function: callable.into(),
                        args: args.clone(),
                        kwargs: Vec::new(),
                        splat_mask: vec![*is_splat; args.len()],
                        kwargs_splat_mask: Vec::new(),
                        span: *span,
                    };
                    self.compile_expr(&ordinary_call)
                }
            }
            Expr::DynamicTypeConstruct {
                base,
                base_expr,
                type_args,
                splat_mask,
                span: _,
            } => {
                // Construct a parametric type at runtime with dynamically evaluated type arguments.
                // Example: Complex{promote_type(T, S)} where T, S are type parameters
                //
                // 1. Compile each type argument expression (evaluates to DataType values)
                // 2. Emit ConstructParametricType[Splat] instruction to build the type

                if let Some(base_expr) = base_expr {
                    self.compile_expr(base_expr)?;
                    for arg in type_args {
                        self.compile_expr(arg)?;
                    }
                    if splat_mask.iter().any(|&b| b) {
                        let mut call_splat_mask = Vec::with_capacity(splat_mask.len() + 1);
                        call_splat_mask.push(false);
                        call_splat_mask.extend(splat_mask.iter().copied());
                        self.emit(Instr::ApplyTypeDynamicSplat(call_splat_mask));
                    } else {
                        self.emit(Instr::ApplyTypeDynamic(type_args.len()));
                    }
                    return Ok(ValueType::DataType);
                }

                for arg in type_args {
                    self.compile_expr(arg)?;
                }

                // Issue #5112: when any argument is a `...`-splat (`Tuple{xs...}`),
                // emit the splat-aware instruction carrying the per-argument mask
                // so the VM flattens splatted collections before construction.
                if splat_mask.iter().any(|&b| b) {
                    self.emit(Instr::ConstructParametricTypeSplat(
                        base.to_string(),
                        splat_mask.clone(),
                    ));
                } else {
                    self.emit(Instr::ConstructParametricType(
                        base.to_string(),
                        type_args.len(),
                    ));
                }
                Ok(ValueType::DataType)
            }
            Expr::QuoteLiteral {
                constructor,
                span: _,
            } => {
                // QuoteLiteral contains an expression that constructs the quoted value.
                // Simply compile the constructor expression which produces the Expr/Symbol.
                self.compile_expr(constructor)
            }
            Expr::AssignExpr {
                var,
                value,
                span: _,
            } => {
                // Assignment as expression: compile the value, assign to variable, leave value on stack
                // This is used for chained assignments like `local result = x = 42`
                // The expression evaluates to the assigned value.
                let value_type = self.compile_expr(value)?;

                // Duplicate the value on stack (one for assignment, one for expression result)
                self.emit(Instr::Dup);

                // Store to variable using the standard store_local method
                self.store_local(var, value_type.clone());

                Ok(value_type)
            }
            Expr::ReturnExpr { value, span } => {
                // Short-circuit `return` is the same non-local transfer as a
                // statement return: pending finally blocks must run before
                // lexical owners are closed (Issue #11569).
                self.compile_stmt(&Stmt::Return {
                    value: value.as_deref().cloned(),
                    span: *span,
                })?;
                // Return expressions never produce a value (control flow exits)
                Ok(ValueType::Nothing)
            }
            Expr::BreakExpr { span: _ } => {
                // Break expression: used in short-circuit context like `cond && break`
                if self.loop_stack.is_empty() {
                    return err("break outside of loop");
                }
                let current_loop_depth = self.loop_stack.len();
                let finally_blocks: Vec<_> = self
                    .finally_stack
                    .iter()
                    .filter(|context| context.loop_depth >= current_loop_depth)
                    .cloned()
                    .collect();
                for context in finally_blocks.iter().rev() {
                    self.compile_pending_finally(context)?;
                }
                self.emit_scope_cleanup_for_loop_exit(current_loop_depth);
                let j_exit = self.here();
                self.emit(Instr::Jump(0xDEAD_BEEF)); // placeholder
                if let Some(loop_ctx) = self.loop_stack.last_mut() {
                    loop_ctx.exit_patches.push(j_exit);
                }
                Ok(ValueType::Nothing)
            }
            Expr::ContinueExpr { span: _ } => {
                // Continue expression: used in short-circuit context like `cond && continue`
                if self.loop_stack.is_empty() {
                    return err("continue outside of loop");
                }
                let current_loop_depth = self.loop_stack.len();
                let finally_blocks: Vec<_> = self
                    .finally_stack
                    .iter()
                    .filter(|context| context.loop_depth >= current_loop_depth)
                    .cloned()
                    .collect();
                for context in finally_blocks.iter().rev() {
                    self.compile_pending_finally(context)?;
                }
                self.emit_scope_cleanup_for_loop_exit(current_loop_depth);
                let j_continue = self.here();
                self.emit(Instr::Jump(0xDEAD_BEEF)); // placeholder
                if let Some(loop_ctx) = self.loop_stack.last_mut() {
                    loop_ctx.continue_patches.push(j_continue);
                }
                Ok(ValueType::Nothing)
            }
        }
    }

    /// Compile a module/main `let` through VM-owned lexical declaration
    /// owners. Each explicit binding is entered only after its RHS has been
    /// evaluated, matching Julia's sequential `let a = rhs, b = rhs` scope
    /// timing; body-assigned locals are declared uninitialized before the
    /// body starts (Issues #11569/#9784).
    fn compile_explicit_let_block(
        &mut self,
        bindings: &[(crate::ir::core::InternedStr, Expr)],
        body: &Block,
        opens_testset_scope: bool,
    ) -> CResult<ValueType> {
        let outer_locals = self.locals.clone();
        let outer_initialized_locals = self.initialized_locals.clone();
        let outer_julia_type_locals = self.julia_type_locals.clone();
        let outer_known_any_rank_array_locals = self.known_any_rank_array_locals.clone();
        let outer_mixed_type_vars = self.mixed_type_vars.clone();
        let outer_declared_globals = self.declared_globals.clone();
        let outer_lexical_scope_locals = self.lexical_scope_locals.clone();
        let outer_local_scope_depth = self.local_scope_depth;

        let mut let_declared_globals = HashSet::new();
        super::stmt::collect_declared_globals(body, &mut let_declared_globals);
        collect_let_body_declared_globals(body, &mut let_declared_globals);

        let explicit_binding_names: HashSet<String> =
            bindings.iter().map(|(name, _)| name.to_string()).collect();
        let mut let_local_names = explicit_binding_names.clone();
        collect_let_body_assignment_names(body, &mut let_local_names);
        let_local_names.extend(
            crate::lowering::soft_scope::ScopeBindingInventory::collect(body)
                .binding_names()
                .cloned(),
        );
        for name in &let_declared_globals {
            let_local_names.remove(name);
        }

        let mut let_lexical_scope_locals = self.lexical_scope_locals.clone();
        let_lexical_scope_locals.extend(let_local_names.iter().cloned());
        crate::lowering::soft_scope::collect_scope_level_bindings(
            body,
            &mut let_lexical_scope_locals,
        );
        for name in &let_declared_globals {
            let_lexical_scope_locals.remove(name);
        }

        self.declared_globals
            .extend(let_declared_globals.iter().cloned());
        self.lexical_scope_locals = let_lexical_scope_locals;
        self.local_scope_depth += 1;

        let mut entered_scope_count = 0usize;
        self.scope_cleanup_stack.push(ScopeCleanupContext {
            names: Vec::new(),
            shadows: Vec::new(),
            lexical_scope_count: 0,
            loop_depth: self.loop_stack.len(),
            cleanup_on_loop_exit: true,
            nonlocal_pop_handler: false,
            nonlocal_pop_caught_exception: false,
        });
        let compile_result = (|| {
            // Sequential binding rule: compile RHS in the previous owner set,
            // then declare/store this binding before compiling the next RHS.
            for (name, value) in bindings {
                let uninitialized = matches!(value, Expr::Literal(Literal::Undef, _));
                let ty = if uninitialized {
                    ValueType::Any
                } else {
                    self.compile_expr(value)?
                };
                if self.enter_explicit_lexical_scope(vec![name.to_string()]) {
                    entered_scope_count += 1;
                    if let Some(cleanup) = self.scope_cleanup_stack.last_mut() {
                        cleanup.lexical_scope_count = entered_scope_count;
                    }
                }
                self.locals.insert(name.to_string(), ty.clone());
                if uninitialized {
                    self.initialized_locals.remove(name.as_str());
                } else {
                    self.initialized_locals.insert(name.to_string());
                    self.store_local(name, ty);
                }
            }

            // An assignment anywhere in the body owns the name throughout the
            // body, including reads that precede the first executed store.
            let mut body_owned_names: Vec<String> = let_local_names
                .iter()
                .filter(|name| !explicit_binding_names.contains(*name))
                .cloned()
                .collect();
            body_owned_names.sort();
            for name in &body_owned_names {
                self.locals.insert(name.clone(), ValueType::Any);
                self.initialized_locals.remove(name);
                self.julia_type_locals.remove(name);
                self.known_any_rank_array_locals.remove(name);
            }
            if self.enter_explicit_lexical_scope(body_owned_names) {
                entered_scope_count += 1;
                if let Some(cleanup) = self.scope_cleanup_stack.last_mut() {
                    cleanup.lexical_scope_count = entered_scope_count;
                }
            }

            self.compile_block_value(body)
        })();
        self.scope_cleanup_stack.pop();

        // The result stays on the operand stack; lexical exits have no stack
        // effect. Emit the normal-path exits in reverse owner order. On a
        // compilation error these instructions are discarded with the failed
        // program, but popping the compile-time stack still restores invariants.
        for _ in 0..entered_scope_count {
            self.exit_explicit_lexical_scope();
        }

        self.locals = outer_locals;
        self.initialized_locals = outer_initialized_locals;
        self.julia_type_locals = outer_julia_type_locals;
        self.known_any_rank_array_locals = outer_known_any_rank_array_locals;
        self.mixed_type_vars = outer_mixed_type_vars;
        self.declared_globals = outer_declared_globals;
        self.lexical_scope_locals = outer_lexical_scope_locals;
        self.local_scope_depth = outer_local_scope_depth;

        // Testset/global declarations intentionally survive the lexical body.
        // Keep their post-scope type conservative for subsequent source-order
        // compilation, matching the legacy path's bookkeeping.
        if opens_testset_scope {
            let mut testset_globals = HashSet::new();
            collect_declared_globals_in_testset_scope(body, &mut testset_globals);
            for name in testset_globals {
                self.locals.insert(name.clone(), ValueType::Any);
                self.julia_type_locals.remove(&name);
                self.known_any_rank_array_locals.remove(&name);
                self.mixed_type_vars.insert(name);
            }
        }

        compile_result
    }

    fn compile_block_value(&mut self, block: &Block) -> CResult<ValueType> {
        let stmts = &block.stmts;
        if stmts.is_empty() {
            self.emit(Instr::PushNothing);
            return Ok(ValueType::Nothing);
        }

        for stmt in stmts.iter().take(stmts.len() - 1) {
            self.compile_stmt(stmt)?;
        }

        match &stmts[stmts.len() - 1] {
            Stmt::Expr { expr, .. } => self.compile_expr(expr),
            Stmt::Block(block) => {
                // A residual tuple-destructuring decomposition (dependent
                // literal, nested, or rest pattern) must
                // yield the destructured RHS value, not the last per-target
                // assignment's value that blind recursion would produce
                // (Issue #10431).
                match crate::lowering::expr::destructuring_tail_value(&block.stmts) {
                    Some(value_expr) => {
                        for stmt in &block.stmts {
                            self.compile_stmt(stmt)?;
                        }
                        self.compile_expr(&value_expr)
                    }
                    None => self.compile_block_value(block),
                }
            }
            // A trailing control-flow statement in value position yields the
            // value of the branch that executed (upstream "the last statement of
            // a block is its value"). A begin/`let` block lowers to a value
            // `LetBlock`, so `x = begin …; if c; a; else; b; end; end` and the
            // `try`/`catch` form must produce the branch value, not `nothing`
            // (Issue #9358 — a `try`/`catch` used as the expression value of a
            // generator/comprehension body). `try_stmt_into_value_expr` /
            // `if_stmt_into_value_expr` rewrite the branch tails to a fresh
            // result variable and read it back. Loops (`for`/`while`) and other
            // trailing statements keep their `nothing` value via the fall-through
            // arm, matching Julia.
            last @ Stmt::Try { span, .. } => {
                match crate::lowering::expr::try_stmt_into_value_expr(last.clone(), *span) {
                    Some(value_expr) => self.compile_expr(&value_expr),
                    None => {
                        self.compile_stmt(last)?;
                        self.emit(Instr::PushNothing);
                        Ok(ValueType::Nothing)
                    }
                }
            }
            last @ Stmt::If { span, .. } => {
                match crate::lowering::expr::if_stmt_into_value_expr(last.clone(), *span) {
                    Some(value_expr) => self.compile_expr(&value_expr),
                    None => {
                        self.compile_stmt(last)?;
                        self.emit(Instr::PushNothing);
                        Ok(ValueType::Nothing)
                    }
                }
            }
            // Julia: an assignment expression evaluates to the assigned value.
            // Mirrors the `Stmt::Assign`/`Stmt::AddAssign` tail-return handling
            // already applied to `compile_function_body`/
            // `compile_block_with_implicit_return` for Issues #8976/#10023, and
            // to `assign_block_tail_value` for Issue #10074: without this arm,
            // a `begin ... end` block (which lowers to this empty-binding
            // `Expr::LetBlock` form) ending in a plain assignment left the
            // block's value at `nothing` instead of the assigned value —
            // reachable both directly (`x = begin y = 1 end`) and as a nested
            // tail block inside a `try`/`catch`/`if` branch (Issue #10074).
            last @ (Stmt::Assign { var, .. } | Stmt::AddAssign { var, .. }) => {
                self.compile_stmt(last)?;
                let loaded_ty = if self.declared_globals.contains(var) {
                    // Declared globals are always loaded as Any (LoadGlobalAny).
                    ValueType::Any
                } else {
                    self.locals
                        .get(var.as_str())
                        .cloned()
                        .unwrap_or(ValueType::Any)
                };
                self.load_local(var)?;
                Ok(loaded_ty)
            }
            last @ Stmt::DestructuringAssign { .. } => {
                match crate::lowering::expr::split_destructuring_stmt_via_temp(last.clone()) {
                    Some((tmp, init, store)) => {
                        self.compile_stmt(&init)?;
                        self.compile_stmt(&store)?;
                        let loaded_ty = self.locals.get(&tmp).cloned().unwrap_or(ValueType::Any);
                        self.load_local(&tmp)?;
                        Ok(loaded_ty)
                    }
                    None => unreachable!("matched DestructuringAssign must split"),
                }
            }
            // Same rule as `Stmt::Assign`/`Stmt::AddAssign` above, extended to
            // indexed/field/dict targets (`v[i] = x`, `obj.field = x`,
            // `d[k] = x`, and their `+=`-desugared shapes), which have no
            // single named variable to reload afterward (Issue #10431).
            // `split_assign_stmt_via_temp` binds the RHS to a fresh
            // compiler-internal temporary and rewrites the store to use it,
            // so the RHS/index expressions are each evaluated exactly once.
            last @ (Stmt::IndexAssign { .. }
            | Stmt::FieldAssign { .. }
            | Stmt::DictAssign { .. }) => {
                match crate::lowering::expr::split_assign_stmt_via_temp(last.clone()) {
                    Some((tmp, init, store)) => {
                        self.compile_stmt(&init)?;
                        self.compile_stmt(&store)?;
                        let loaded_ty = self.locals.get(&tmp).cloned().unwrap_or(ValueType::Any);
                        self.load_local(&tmp)?;
                        Ok(loaded_ty)
                    }
                    None => {
                        // Unreachable given the match arm above only matches
                        // shapes `split_assign_stmt_via_temp` handles; stay
                        // defensive rather than panicking.
                        self.compile_stmt(last)?;
                        self.emit(Instr::PushNothing);
                        Ok(ValueType::Nothing)
                    }
                }
            }
            last => {
                self.compile_stmt(last)?;
                self.emit(Instr::PushNothing);
                Ok(ValueType::Nothing)
            }
        }
    }

    pub(super) fn load_local(&mut self, name: &str) -> CResult<()> {
        // A name declared `global x` reads from the module-level (frame 0)
        // binding. Use an explicit global load so slotization cannot rewrite
        // the read to a stale local/testset slot after `StoreGlobalAny` changes
        // the global value's runtime type (Issue #6269).
        if self.declared_globals.contains(name) {
            self.emit_load_declared_global(name);
            return Ok(());
        }

        // Check if this is a captured variable from a closure's outer scope
        if self.captured_vars.contains(name) {
            self.emit(Instr::LoadCaptured(name.to_string()));
            return Ok(());
        }

        // Resolve module constants to qualified names (both in module body and function context)
        // This matches store_local behavior which stores module constants with qualified names
        let (load_name, is_module_constant) = if !self.locals.contains_key(name) {
            // Variable not in locals - check if this is a module constant
            if let Some(module_path) = &self.current_module_path {
                if let Some(const_names) = self.module_constants.get(module_path) {
                    if const_names.contains(name) {
                        (format!("{}.{}", module_path, name), true)
                    } else {
                        (name.to_string(), false)
                    }
                } else {
                    (name.to_string(), false)
                }
            } else {
                (name.to_string(), false)
            }
        } else {
            (name.to_string(), false)
        };

        // For module constants, use the qualified module-level binding.
        if is_module_constant {
            self.emit(Instr::LoadGlobalAny(load_name));
            return Ok(());
        }

        if let Some(qualified) = self.module_private_type_object_name(name) {
            self.emit(Instr::PushDataType(qualified));
            return Ok(());
        }

        // Abstract numeric parameters (`x::Number`, `x::Real`, `x::Integer`, ...)
        // can receive BigInt/BigFloat at runtime. Loading them through the
        // F64/I64 slot selected by the annotation would reject those values before
        // dynamic numeric dispatch has a chance to run (Issue #2498/#4337).
        if self.abstract_numeric_params.contains(name) {
            self.emit(Instr::LoadAny(load_name));
            return Ok(());
        }

        // Prefer local type, fall back to global type (for top-level const/global variables),
        // then default to Any. This ensures functions can access prelude consts like arrays.
        let ty = self
            .locals
            .get(name)
            .cloned()
            .or_else(|| self.shared_ctx.global_types.get(name).cloned())
            .unwrap_or(ValueType::Any);
        if !self.locals.contains_key(name)
            && self.shared_ctx.global_types.contains_key(name)
            && matches!(ty, ValueType::Array | ValueType::ArrayOf(_, _))
        {
            self.emit(Instr::LoadGlobalAny(load_name));
            return Ok(());
        }
        self.emit(match ty {
            ValueType::I64 => Instr::LoadI64(load_name.clone()),
            ValueType::F64 => Instr::LoadF64(load_name.clone()),
            ValueType::F32 => Instr::LoadF32(load_name.clone()),
            ValueType::F16 => Instr::LoadF16(load_name.clone()),
            ValueType::Bool => Instr::LoadBool(load_name.clone()),
            ValueType::Array | ValueType::ArrayOf(_, _) => Instr::LoadArray(load_name.clone()),
            ValueType::Str => Instr::LoadStr(load_name.clone()),
            ValueType::Nothing => Instr::PushNothing, // Nothing is a singleton
            ValueType::Struct(_) => Instr::LoadStruct(load_name.clone()), // All structs including Complex
            ValueType::Rng => Instr::LoadRng(load_name.clone()),
            ValueType::Range => Instr::LoadRange(load_name.clone()),
            ValueType::Tuple => Instr::LoadTuple(load_name.clone()),
            ValueType::NamedTuple => Instr::LoadNamedTuple(load_name.clone()),
            ValueType::Dict => Instr::LoadDict(load_name.clone()),
            // All other types use LoadAny
            _ => Instr::LoadAny(load_name),
        });
        Ok(())
    }

    pub(super) fn store_local(&mut self, name: &str, ty: ValueType) {
        // A name declared `global x` inside a function writes to the module-level
        // (frame 0) binding and must NOT introduce a local slot, so that later
        // reads fall through to the global and the top-level binding is updated
        // (Issues #5548, #5549, #11312). `StoreGlobalAny` always targets frame 0.
        if self.declared_globals.contains(name) {
            self.emit_store_declared_global(name);
            return;
        }

        // An active explicit lexical declaration wins over a same-named
        // module constant. Clause scopes (`try`/`catch`/`finally`) can live at
        // module compiler depth zero, so `local x` must be routed here before
        // the qualified module-constant store below (Issue #11569).
        if self.explicit_lexical_owner_active(name) {
            self.locals.insert(name.to_string(), ty.clone());
            self.initialized_locals.insert(name.to_string());
            let instr = match ty {
                ValueType::I64 => Instr::StoreI64(name.to_string()),
                ValueType::F64 => Instr::StoreF64(name.to_string()),
                ValueType::F32 => Instr::StoreF32(name.to_string()),
                ValueType::F16 => Instr::StoreF16(name.to_string()),
                ValueType::Bool => Instr::StoreBool(name.to_string()),
                ValueType::Array | ValueType::ArrayOf(_, _) => Instr::StoreArray(name.to_string()),
                ValueType::Str => Instr::StoreStr(name.to_string()),
                ValueType::Struct(_) => Instr::StoreStruct(name.to_string()),
                ValueType::Rng => Instr::StoreRng(name.to_string()),
                ValueType::Range => Instr::StoreRange(name.to_string()),
                ValueType::Tuple => Instr::StoreTuple(name.to_string()),
                ValueType::NamedTuple => Instr::StoreNamedTuple(name.to_string()),
                ValueType::Dict => Instr::StoreDict(name.to_string()),
                ValueType::Set => Instr::StoreSet(name.to_string()),
                _ => Instr::StoreAny(name.to_string()),
            };
            self.emit(instr);
            return;
        }

        // In module body context (not function), store constants with qualified names
        // so they can be accessed from module functions
        let (store_name, is_module_constant) =
            if !self.strict_undefined_check && self.local_scope_depth == 0 {
                // Module body context - check if this is a module constant
                if let Some(module_path) = &self.current_module_path {
                    if let Some(const_names) = self.module_constants.get(module_path) {
                        if const_names.contains(name) {
                            (format!("{}.{}", module_path, name), true)
                        } else {
                            (name.to_string(), false)
                        }
                    } else {
                        (name.to_string(), false)
                    }
                } else {
                    (name.to_string(), false)
                }
            } else {
                (name.to_string(), false)
            };

        // Don't insert module constants into locals - they're stored in the global frame
        // with qualified names and will be resolved via module_constants lookup
        if !is_module_constant {
            self.locals.insert(name.to_string(), ty.clone());
            self.initialized_locals.insert(name.to_string());
        }
        match ty {
            ValueType::Nothing => {
                // A singleton still needs a physical slot. A later assignment
                // in only one control-flow branch can widen this local from
                // Nothing to Any; if the initial value is merely popped, the
                // non-assigning branch reaches the widened LoadSlot with no
                // backing value and raises UndefVarError (Issue #10819).
                // Materializing every Nothing assignment keeps representation
                // changes transactional across branches. Reads that remain
                // statically Nothing may still use the PushNothing fast path.
                if is_module_constant {
                    self.emit(Instr::StoreGlobalAny(store_name));
                } else {
                    self.emit(Instr::StoreAny(store_name));
                }
            }
            _ => {
                // Module constants live in the module-level frame under their
                // qualified name so module functions can resolve them.
                if is_module_constant {
                    self.emit(Instr::StoreGlobalAny(store_name));
                    return;
                }

                let instr = match ty {
                    ValueType::I64 => Instr::StoreI64(store_name.clone()),
                    ValueType::F64 => Instr::StoreF64(store_name.clone()),
                    ValueType::F32 => Instr::StoreF32(store_name.clone()),
                    ValueType::F16 => Instr::StoreF16(store_name.clone()),
                    ValueType::Bool => Instr::StoreBool(store_name.clone()),
                    ValueType::Array | ValueType::ArrayOf(_, _) => {
                        Instr::StoreArray(store_name.clone())
                    }
                    ValueType::Str => Instr::StoreStr(store_name.clone()),
                    ValueType::Struct(_) => Instr::StoreStruct(store_name.clone()), // All structs including Complex
                    ValueType::Rng => Instr::StoreRng(store_name.clone()),
                    ValueType::Range => Instr::StoreRange(store_name.clone()),
                    ValueType::Tuple => Instr::StoreTuple(store_name.clone()),
                    ValueType::NamedTuple => Instr::StoreNamedTuple(store_name.clone()),
                    ValueType::Dict => Instr::StoreDict(store_name.clone()),
                    ValueType::Set => Instr::StoreSet(store_name.clone()),
                    // All other types use StoreAny
                    _ => Instr::StoreAny(store_name),
                };
                self.emit(instr)
            }
        }
    }
}
