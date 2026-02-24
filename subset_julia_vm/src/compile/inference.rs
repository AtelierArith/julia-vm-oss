//! Type inference engine for the compiler.
//!
//! This module provides functions to infer types of expressions, function return types,
//! and parameter types from usage patterns.
//!
//! # Architecture
//!
//! The type inference system uses a two-tier approach:
//!
//! 1. **Legacy Inference (`infer_function_return_type`)**: Simple forward analysis
//!    based on expression types and local variable tracking. This is maintained
//!    for backward compatibility.
//!
//! 2. **Shared Inference Engine (`build_shared_inference_engine`)**: Lattice-based
//!    abstract interpretation with support for:
//!    - Loop variable type inference from iterators
//!    - Conditional type narrowing (isa checks, === nothing)
//!    - Union type inference
//!    - Transfer functions for built-in operations
//!    - HOF (Higher-Order Function) inference via interprocedural analysis
//!
//! # Usage
//!
//! The main compiler uses `build_shared_inference_engine` to create a single engine
//! before the function compilation loop, then calls `engine.infer_function()` for each
//! function. This provides more accurate type information, especially for:
//!
//! - Loop variables in `for` loops
//! - Type narrowing in conditional branches
//! - Union types from multiple return paths
//!
//! # Design Principles
//!
//! - **Abstract Interpretation**: Simulate program execution using abstract types
//! - **Fixed-Point Iteration**: Iterate until type information stabilizes
//! - **Type Lattice**: Organize types in a hierarchy (Bottom → Concrete → Union → Top)
//! - **Transfer Functions**: Built-in functions have known return types
//!
//! See `docs/vm/TYPE_INFERENCE.md` for user-facing documentation.

use std::collections::{HashMap, HashSet};

use crate::ir::core::{BinaryOp, BuiltinOp, Expr, Function, Literal, Stmt};
use crate::vm::{ArrayElementType, ValueType};

use super::promotion::{promote_complex, promote_type};
use super::{is_pi_name, StructInfo};

/// Collect local variable types from assignments for type inference.
/// When a variable is assigned different types:
/// - F64 + I64 mix: use Any for true dynamic typing (Julia semantics)
/// - Struct + Any: keep Struct (more specific)
/// - Otherwise: use the new type
fn widen_type(old: &Option<ValueType>, new: ValueType) -> ValueType {
    match (old, &new) {
        // F64 + I64 mix: use Any for dynamic typing
        // This enables Julia-compatible behavior where die = 7.0 then die = 6
        // correctly changes the type at runtime instead of widening to F64
        (Some(ValueType::F64), ValueType::I64) | (Some(ValueType::I64), ValueType::F64) => {
            ValueType::Any
        }
        // Once Any (from F64+I64 mix), stay Any for subsequent numeric assignments
        // This ensures the variable remains dynamically typed throughout the function
        (Some(ValueType::Any), ValueType::I64) | (Some(ValueType::Any), ValueType::F64) => {
            ValueType::Any
        }
        // If old is Struct and new is Any, keep Struct (more specific)
        // This preserves struct types from REPL session when inject_globals creates Literal::Struct
        (Some(ValueType::Struct(id)), ValueType::Any) => ValueType::Struct(*id),
        // Complex numbers are now Pure Julia structs - no automatic widening
        // Otherwise use the new type
        _ => new,
    }
}

/// Helper function to get struct name from ValueType::Struct
fn get_struct_name(
    vt: &ValueType,
    type_id_to_name: &HashMap<usize, String>,
) -> Option<String> {
    if let ValueType::Struct(type_id) = vt {
        return type_id_to_name.get(type_id).cloned();
    }
    None
}

/// Build a reverse lookup map from type_id to struct name (Issue #3358).
/// This replaces O(n) iteration over struct_table with O(1) HashMap lookup.
fn build_type_id_to_name(struct_table: &HashMap<String, StructInfo>) -> HashMap<usize, String> {
    struct_table
        .iter()
        .map(|(name, info)| (info.type_id, name.clone()))
        .collect()
}

/// Convert ValueType to type name string for promotion.
fn value_type_to_type_name(vt: &ValueType) -> &'static str {
    match vt {
        ValueType::F16 => "Float16",
        ValueType::F64 => "Float64",
        ValueType::F32 => "Float32",
        ValueType::BigFloat => "BigFloat",
        ValueType::I64 => "Int64",
        ValueType::I32 => "Int32",
        ValueType::I16 => "Int16",
        ValueType::I8 => "Int8",
        ValueType::I128 => "Int128",
        ValueType::BigInt => "BigInt",
        ValueType::U8 => "UInt8",
        ValueType::U16 => "UInt16",
        ValueType::U32 => "UInt32",
        ValueType::U64 => "UInt64",
        ValueType::U128 => "UInt128",
        ValueType::Bool => "Bool",
        _ => "Any",
    }
}

fn type_name_to_value_type(name: &str) -> Option<ValueType> {
    match name {
        "Int8" => Some(ValueType::I8),
        "Int16" => Some(ValueType::I16),
        "Int32" => Some(ValueType::I32),
        "Int64" | "Int" => Some(ValueType::I64),
        "Int128" => Some(ValueType::I128),
        "BigInt" => Some(ValueType::BigInt),
        "UInt8" => Some(ValueType::U8),
        "UInt16" => Some(ValueType::U16),
        "UInt32" => Some(ValueType::U32),
        "UInt64" | "UInt" => Some(ValueType::U64),
        "UInt128" => Some(ValueType::U128),
        "Float16" => Some(ValueType::F16),
        "Float32" => Some(ValueType::F32),
        "Float64" => Some(ValueType::F64),
        "BigFloat" => Some(ValueType::BigFloat),
        "Bool" => Some(ValueType::Bool),
        "Any" => Some(ValueType::Any),
        _ => None,
    }
}

fn is_numeric_value_type(vt: &ValueType) -> bool {
    matches!(
        vt,
        ValueType::I8
            | ValueType::I16
            | ValueType::I32
            | ValueType::I64
            | ValueType::I128
            | ValueType::BigInt
            | ValueType::U8
            | ValueType::U16
            | ValueType::U32
            | ValueType::U64
            | ValueType::U128
            | ValueType::F16
            | ValueType::F32
            | ValueType::F64
            | ValueType::BigFloat
            | ValueType::Bool
    )
}

pub(crate) fn promote_numeric_value_types(
    left: &ValueType,
    right: &ValueType,
) -> Option<ValueType> {
    if !is_numeric_value_type(left) || !is_numeric_value_type(right) {
        return None;
    }
    let left_name = value_type_to_type_name(left);
    let right_name = value_type_to_type_name(right);
    if left_name == "Any" || right_name == "Any" {
        return None;
    }
    let promoted = promote_type(left_name, right_name);
    type_name_to_value_type(&promoted)
}

/// Find a Complex type in the struct table, or return None.
fn find_complex_type_in_table(
    complex_name: &str,
    struct_table: &HashMap<String, StructInfo>,
) -> Option<ValueType> {
    if let Some(info) = struct_table.get(complex_name) {
        return Some(ValueType::Struct(info.type_id));
    }
    None
}

/// Collect local variable types and track variables with mixed F64+I64 types.
/// These variables should use dynamic typing (StoreAny/LoadAny) to allow type changes at runtime.
pub fn collect_local_types_with_mixed_tracking(
    stmts: &[Stmt],
    locals: &mut HashMap<String, ValueType>,
    protected: &HashSet<String>,
    struct_table: &HashMap<String, StructInfo>,
    global_types: &HashMap<String, ValueType>,
    mixed_type_vars: &mut HashSet<String>,
) {
    collect_local_types_for_inference_with_mixed_tracking(
        stmts,
        locals,
        protected,
        struct_table,
        global_types,
        true,
        mixed_type_vars,
    )
}

/// Collect local variable types with widening and track mixed-type variables.
/// Variables with mixed F64+I64 types are tracked in mixed_type_vars for dynamic typing.
fn collect_local_types_for_inference_with_mixed_tracking(
    stmts: &[Stmt],
    locals: &mut HashMap<String, ValueType>,
    protected: &HashSet<String>,
    struct_table: &HashMap<String, StructInfo>,
    global_types: &HashMap<String, ValueType>,
    use_widening: bool,
    mixed_type_vars: &mut HashSet<String>,
) {
    for stmt in stmts {
        match stmt {
            Stmt::Assign { var, value, .. } => {
                // Skip updating protected variables (function parameters)
                // This prevents overwriting parameter types inferred from assignments
                if protected.contains(var) {
                    continue;
                }

                // Use struct-aware type inference to properly track Complex and other struct types
                let ty = infer_value_type_with_structs(value, locals, struct_table, global_types);
                if use_widening {
                    // Check if this is a DIRECT literal assignment (not a compound assignment)
                    // Only direct literal assignments can cause mixed_type_vars (dynamic typing).
                    // Compound assignments like `x *= y` use type promotion, not dynamic typing.
                    let is_direct_literal = matches!(
                        value,
                        Expr::Literal(Literal::Int(_) | Literal::Float(_) | Literal::Float32(_) | Literal::Float16(_), _)
                    );

                    // For function bodies, use widening to handle control flow where a variable
                    // might have different types in different branches (e.g., die = floor(x) vs die = 6).
                    let old_ty = locals.get(var).cloned();
                    let widened = widen_type(&old_ty, ty.clone());
                    // Track variables that were widened to Any due to F64+I64 mix
                    // BUT only for direct literal assignments, not compound assignments.
                    // This allows Julia semantics: `die = 7.0; die = 6` preserves Int64 at runtime,
                    // while `result = 1; result *= 2.0` uses type promotion to Float64.
                    if widened == ValueType::Any && old_ty.is_some() && is_direct_literal {
                        if let Some(ref old) = old_ty {
                            if (*old == ValueType::F64 && ty == ValueType::I64)
                                || (*old == ValueType::I64 && ty == ValueType::F64)
                            {
                                mixed_type_vars.insert(var.clone());
                            }
                        }
                    }
                    locals.insert(var.clone(), widened);
                } else {
                    // For main block/REPL, use exact types (Julia semantics)
                    locals.insert(var.clone(), ty);
                }
            }
            Stmt::For { var, body, .. } => {
                // Register the loop variable as I64 (it's the loop counter)
                if !protected.contains(var) {
                    locals.insert(var.clone(), ValueType::I64);
                }
                collect_local_types_for_inference_with_mixed_tracking(
                    &body.stmts,
                    locals,
                    protected,
                    struct_table,
                    global_types,
                    use_widening,
                    mixed_type_vars,
                );
            }
            Stmt::ForEach {
                var,
                iterable,
                body,
                ..
            } => {
                // Infer element type from the iterable using v2 inference
                if !protected.contains(var) {
                    // Get the type of the iterable expression
                    let iterable_type =
                        infer_value_type_with_structs(iterable, locals, struct_table, global_types);
                    // Convert to lattice type and extract element type
                    let lattice_iterable =
                        crate::compile::lattice::types::LatticeType::from(&iterable_type);
                    let elem_lattice = crate::compile::abstract_interp::loop_analysis::element_type(
                        &lattice_iterable,
                    );
                    let elem_type = crate::compile::bridge::lattice_to_value_type(&elem_lattice);
                    locals.insert(var.clone(), elem_type);
                }
                collect_local_types_for_inference_with_mixed_tracking(
                    &body.stmts,
                    locals,
                    protected,
                    struct_table,
                    global_types,
                    use_widening,
                    mixed_type_vars,
                );
            }
            Stmt::While { body, .. } | Stmt::Timed { body, .. } => {
                collect_local_types_for_inference_with_mixed_tracking(
                    &body.stmts,
                    locals,
                    protected,
                    struct_table,
                    global_types,
                    use_widening,
                    mixed_type_vars,
                );
            }
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_local_types_for_inference_with_mixed_tracking(
                    &then_branch.stmts,
                    locals,
                    protected,
                    struct_table,
                    global_types,
                    use_widening,
                    mixed_type_vars,
                );
                if let Some(eb) = else_branch {
                    collect_local_types_for_inference_with_mixed_tracking(
                        &eb.stmts,
                        locals,
                        protected,
                        struct_table,
                        global_types,
                        use_widening,
                        mixed_type_vars,
                    );
                }
            }
            Stmt::Try {
                try_block,
                catch_block,
                else_block,
                finally_block,
                ..
            } => {
                collect_local_types_for_inference_with_mixed_tracking(
                    &try_block.stmts,
                    locals,
                    protected,
                    struct_table,
                    global_types,
                    use_widening,
                    mixed_type_vars,
                );
                if let Some(cb) = catch_block {
                    collect_local_types_for_inference_with_mixed_tracking(
                        &cb.stmts,
                        locals,
                        protected,
                        struct_table,
                        global_types,
                        use_widening,
                        mixed_type_vars,
                    );
                }
                if let Some(eb) = else_block {
                    collect_local_types_for_inference_with_mixed_tracking(
                        &eb.stmts,
                        locals,
                        protected,
                        struct_table,
                        global_types,
                        use_widening,
                        mixed_type_vars,
                    );
                }
                if let Some(fb) = finally_block {
                    collect_local_types_for_inference_with_mixed_tracking(
                        &fb.stmts,
                        locals,
                        protected,
                        struct_table,
                        global_types,
                        use_widening,
                        mixed_type_vars,
                    );
                }
            }
            // @testset: collect local types from their bodies (Issue #2358)
            // Variables defined inside @testset blocks should be available for closure capture
            Stmt::TestSet { body, .. } => {
                collect_local_types_for_inference_with_mixed_tracking(
                    &body.stmts,
                    locals,
                    protected,
                    struct_table,
                    global_types,
                    use_widening,
                    mixed_type_vars,
                );
            }
            // Handle Stmt::Expr containing LetBlock - these appear from macro expansions
            // like @testset where the body is wrapped in nested LetBlocks (Issue #2358)
            Stmt::Expr { expr, .. } => {
                collect_expr_locals(
                    expr,
                    locals,
                    protected,
                    struct_table,
                    global_types,
                    use_widening,
                    mixed_type_vars,
                );
            }
            _ => {}
        }
    }
}

/// Recursively collect local variable types from expressions (Issue #2358).
/// This handles LetBlock expressions that contain statements, which appear from
/// macro expansions like @testset where the body is wrapped in nested LetBlocks.
fn collect_expr_locals(
    expr: &Expr,
    locals: &mut HashMap<String, ValueType>,
    protected: &HashSet<String>,
    struct_table: &HashMap<String, StructInfo>,
    global_types: &HashMap<String, ValueType>,
    use_widening: bool,
    mixed_type_vars: &mut HashSet<String>,
) {
    match expr {
        Expr::LetBlock { body, .. } => {
            // Recurse into the LetBlock's body statements
            collect_local_types_for_inference_with_mixed_tracking(
                &body.stmts,
                locals,
                protected,
                struct_table,
                global_types,
                use_widening,
                mixed_type_vars,
            );
        }
        Expr::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            // Handle ternary expressions that might contain LetBlocks
            collect_expr_locals(
                then_expr,
                locals,
                protected,
                struct_table,
                global_types,
                use_widening,
                mixed_type_vars,
            );
            collect_expr_locals(
                else_expr,
                locals,
                protected,
                struct_table,
                global_types,
                use_widening,
                mixed_type_vars,
            );
        }
        Expr::Call { args, .. } => {
            // Handle call arguments that might contain LetBlocks
            for arg in args {
                collect_expr_locals(
                    arg,
                    locals,
                    protected,
                    struct_table,
                    global_types,
                    use_widening,
                    mixed_type_vars,
                );
            }
        }
        _ => {
            // Other expressions don't contain statements
        }
    }
}

/// Collect local variable types from assignments with struct-awareness for global types.
/// This version uses `infer_value_type_with_structs` to properly resolve struct constructor calls.
/// Also collects const struct constructors for inlining in functions.
pub fn collect_global_types_for_inference(
    stmts: &[Stmt],
    globals: &mut HashMap<String, ValueType>,
    struct_table: &HashMap<String, StructInfo>,
    const_structs: &mut HashMap<String, (String, usize, usize)>,
) {
    for stmt in stmts {
        if let Stmt::Assign { var, value, .. } = stmt {
            // Pass globals as both locals and global_types: globals already accumulates
            // previously seen global assignments, so it serves as the global lookup too.
            let ty = infer_value_type_with_structs(value, globals, struct_table, globals);
            // For global/top-level assignments, use the exact inferred type.
            // Julia variables can change type on reassignment (dynamic typing).
            // Unlike local variable analysis where widening helps unify types
            // across control flow, global assignments should reflect the actual type.
            globals.insert(var.clone(), ty);

            // Check if this is an empty struct constructor call for const inlining
            // e.g., `const M = MyType()` - only inline when args is empty
            // For non-empty structs like `im = Complex{Bool}(false, true)`, we need
            // to load the actual global value, not inline the constructor
            if let Expr::Call { function, args, .. } = value {
                if args.is_empty() {
                    if let Some(struct_info) = struct_table.get(function) {
                        // Store (struct_name, type_id, field_count) for inlining
                        const_structs
                            .insert(var.clone(), (function.clone(), struct_info.type_id, 0));
                    }
                }
            }
        }
    }
}

/// Infer the ValueType of an expression given local variable types.
/// This is an internal helper function used by the inference engine.
fn infer_value_type(expr: &Expr, locals: &HashMap<String, ValueType>) -> ValueType {
    match expr {
        Expr::Literal(lit, _) => match lit {
            Literal::Int(_) => ValueType::I64,
            Literal::Int128(_) => ValueType::I128,
            Literal::BigInt(_) => ValueType::BigInt,
            Literal::BigFloat(_) => ValueType::BigFloat,
            Literal::Float(_) => ValueType::F64,
            Literal::Float32(_) => ValueType::F32,
            Literal::Float16(_) => ValueType::F16,
            Literal::Str(_) => ValueType::Str,
            Literal::Char(_) => ValueType::Char,
            Literal::Bool(_) => ValueType::Bool,
            Literal::Nothing => ValueType::Nothing,
            Literal::Missing => ValueType::Missing,
            Literal::Module(_) => ValueType::Module,
            Literal::Array(_, _) => ValueType::ArrayOf(crate::vm::value::ArrayElementType::F64),
            Literal::ArrayI64(_, _) => ValueType::ArrayOf(crate::vm::value::ArrayElementType::I64),
            Literal::ArrayBool(_, _) => {
                ValueType::ArrayOf(crate::vm::value::ArrayElementType::Bool)
            }
            Literal::Struct(_, _) => ValueType::Any, // Type will be resolved during compilation
            Literal::Undef => ValueType::Any,        // Required kwarg marker
            // Metaprogramming literals
            Literal::Symbol(_) => ValueType::Any,
            Literal::Expr { .. } => ValueType::Any,
            Literal::QuoteNode(_) => ValueType::Any,
            Literal::LineNumberNode { .. } => ValueType::Any,
            // Regex literal
            Literal::Regex { .. } => ValueType::Regex,
            // Enum literal
            Literal::Enum { .. } => ValueType::Enum,
        },
        Expr::Var(name, _) => {
            if let Some(ty) = locals.get(name) {
                ty.clone()
            } else if is_pi_name(name) {
                ValueType::F64
            } else {
                // Default to Any (not I64) to ensure dynamic dispatch for unknown types
                ValueType::Any
            }
        }
        Expr::BinaryOp {
            op, left, right, ..
        } => {
            let lt = infer_value_type(left, locals);
            let rt = infer_value_type(right, locals);
            // Without struct_table, we can't resolve Complex type_id
            // If either operand is Any (could be Complex), result could be Any
            let is_any = lt == ValueType::Any || rt == ValueType::Any;
            match op {
                // Comparisons return Bool (Julia semantics)
                BinaryOp::Lt
                | BinaryOp::Gt
                | BinaryOp::Le
                | BinaryOp::Ge
                | BinaryOp::Eq
                | BinaryOp::Ne => ValueType::Bool,
                // Logical operators return Bool
                BinaryOp::And | BinaryOp::Or => ValueType::Bool,
                // Division with any operands return Any (could be Complex)
                BinaryOp::Div => {
                    if is_any {
                        ValueType::Any
                    } else {
                        ValueType::F64
                    }
                }
                // Power operator: Int^Int -> Int, otherwise -> Float64/Any (Julia semantics)
                BinaryOp::Pow => {
                    if is_any {
                        ValueType::Any
                    } else if lt == ValueType::I64 && rt == ValueType::I64 {
                        ValueType::I64
                    } else {
                        ValueType::F64
                    }
                }
                // Mul with strings is string concatenation
                BinaryOp::Mul => {
                    if lt == ValueType::Str || rt == ValueType::Str {
                        ValueType::Str
                    } else if let Some(promoted) = promote_numeric_value_types(&lt, &rt) {
                        promoted
                    } else if is_any {
                        ValueType::Any
                    } else if lt == ValueType::F64 || rt == ValueType::F64 {
                        ValueType::F64
                    } else {
                        ValueType::I64
                    }
                }
                // Other arithmetic operations
                _ => {
                    if let Some(promoted) = promote_numeric_value_types(&lt, &rt) {
                        promoted
                    } else if is_any {
                        ValueType::Any
                    } else if lt == ValueType::F64 || rt == ValueType::F64 {
                        ValueType::F64
                    } else {
                        ValueType::I64
                    }
                }
            }
        }
        Expr::ArrayLiteral { .. }
        | Expr::Comprehension { .. }
        | Expr::MultiComprehension { .. } => ValueType::Array,
        Expr::TypedEmptyArray { element_type, .. } => {
            // Typed empty array like Bool[], Int64[], Float64[]
            match element_type.as_str() {
                "Int" | "Int64" => ValueType::ArrayOf(ArrayElementType::I64),
                "Int32" => ValueType::ArrayOf(ArrayElementType::I64),
                "Float64" | "Float32" => ValueType::ArrayOf(ArrayElementType::F64),
                "Bool" => ValueType::ArrayOf(ArrayElementType::Bool),
                "String" => ValueType::ArrayOf(ArrayElementType::String),
                "Char" => ValueType::ArrayOf(ArrayElementType::Char),
                _ => ValueType::ArrayOf(ArrayElementType::Any),
            }
        }
        Expr::Range { .. } => ValueType::Range,
        Expr::Builtin { name, args, .. } => {
            // Infer return type based on builtin operation
            match name {
                BuiltinOp::Zeros
                | BuiltinOp::Ones
                // Note: Fill, Trues, Falses are now Pure Julia — Issue #2640
                | BuiltinOp::Push
                | BuiltinOp::Pop => ValueType::Array,
                // Note: Adjoint and Transpose have been migrated to Pure Julia
                BuiltinOp::Lu => ValueType::Tuple,
                BuiltinOp::Det => ValueType::F64,
                // Note: Inv removed — dead code (Issue #2643)
                // rand() and randn() with no args return F64, with args return Array
                BuiltinOp::Rand | BuiltinOp::Randn => {
                    if args.is_empty() {
                        ValueType::F64
                    } else {
                        ValueType::Array
                    }
                }
                BuiltinOp::Length
                | BuiltinOp::Size
                | BuiltinOp::TimeNs
                | BuiltinOp::DictGet => ValueType::I64,
                BuiltinOp::HasKey
                | BuiltinOp::Isa
                | BuiltinOp::Isbits
                | BuiltinOp::Isbitstype
                | BuiltinOp::Hasfield
                // Isconcretetype, Isabstracttype, Isprimitivetype, Isstructtype, Ismutabletype
                // removed - now Pure Julia (base/reflection.jl)
                | BuiltinOp::Ismutable => ValueType::Bool,
                BuiltinOp::Sizeof => ValueType::I64,
                BuiltinOp::Supertype => ValueType::DataType,
                BuiltinOp::Sqrt => ValueType::F64,
                // Note: Sum is now Pure Julia (base/array.jl)
                // Complex operations (real, imag, conj, abs, abs2) are now Pure Julia
                BuiltinOp::TupleFirst | BuiltinOp::TupleLast => ValueType::Any, // Tuple element type is unknown
                BuiltinOp::DictKeys | BuiltinOp::DictValues | BuiltinOp::DictPairs => {
                    ValueType::Tuple
                }
                BuiltinOp::DictDelete
                | BuiltinOp::DictMerge
                | BuiltinOp::DictMergeBang
                | BuiltinOp::DictEmpty => ValueType::Dict,
                BuiltinOp::DictGetBang => ValueType::Any, // Returns the value (type depends on dict)
                BuiltinOp::StableRNG | BuiltinOp::XoshiroRNG => ValueType::Rng,
                BuiltinOp::Ref => ValueType::Any,
                BuiltinOp::TypeOf => ValueType::DataType,
                BuiltinOp::Iterate => ValueType::Any, // Returns Tuple or Nothing
                BuiltinOp::Collect => ValueType::Array, // collect always returns Array
                BuiltinOp::Generator => ValueType::Generator, // lazy iterator
                BuiltinOp::SymbolNew => ValueType::Symbol, // Symbol("name")
                BuiltinOp::ExprNew => ValueType::Expr, // Expr(head, args...)
                BuiltinOp::LineNumberNodeNew => ValueType::LineNumberNode, // LineNumberNode(line, file)
                BuiltinOp::QuoteNodeNew => ValueType::QuoteNode,           // QuoteNode(value)
                BuiltinOp::GlobalRefNew => ValueType::GlobalRef,           // GlobalRef(mod, name)
                BuiltinOp::Gensym => ValueType::Symbol, // gensym() or gensym("base")
                BuiltinOp::Esc => ValueType::Expr,      // esc(expr)
                BuiltinOp::Eval => ValueType::Any,      // eval(expr) - result type is dynamic
                BuiltinOp::Zero => {
                    if !args.is_empty() {
                        infer_value_type(&args[0], locals)
                    } else {
                        ValueType::F64
                    }
                }
                BuiltinOp::IfElse => {
                    if args.len() >= 3 {
                        let then_ty = infer_value_type(&args[1], locals);
                        let else_ty = infer_value_type(&args[2], locals);
                        if then_ty == else_ty {
                            then_ty
                        } else if let Some(promoted) = promote_numeric_value_types(&then_ty, &else_ty) {
                            promoted
                        } else {
                            then_ty
                        }
                    } else {
                        ValueType::I64
                    }
                }
                _ => ValueType::I64, // Default for unknown builtins
            }
        }
        Expr::Call { function, args, .. } => {
            // For user-defined function calls, we'd need the method table.
            // Unknown calls default to Any (conservative and type-safe).
            // Built-in function calls like "sqrt", "sin", etc.
            match function.as_str() {
                "sqrt" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan"
                | "exp" | "log" | "floor" | "ceil" | "round"
                // Note: abs and abs2 preserve argument type (Int64 → Int64, Float64 → Float64)
                | "prod" | "max" | "min" | "mean"
                // Note: sum is now Pure Julia and preserves element type
                // Intrinsic wrappers from boot.jl (Float64 operations)
                | "neg_float" | "add_float" | "sub_float" | "mul_float" | "div_float" | "pow_float"
                | "abs_float" | "copysign_float" | "sqrt_llvm" | "floor_llvm" | "ceil_llvm" | "trunc_llvm"
                | "sitofp" => ValueType::F64,
                // Intrinsic wrappers from boot.jl (Int64 operations)
                "neg_int" | "add_int" | "sub_int" | "mul_int" | "sdiv_int" | "srem_int"
                | "and_int" | "or_int" | "xor_int" | "not_int" | "shl_int" | "lshr_int" | "ashr_int"
                | "fptosi" => ValueType::I64,
                // Intrinsic wrappers from boot.jl (Bool operations)
                "eq_int" | "ne_int" | "slt_int" | "sle_int" | "sgt_int" | "sge_int"
                | "eq_float" | "ne_float" | "lt_float" | "le_float" | "gt_float" | "ge_float" => ValueType::Bool,
                // Type constructors/converters
                "Int64" | "Int" | "Int32" | "Int16" | "Int8" => ValueType::I64,
                // Integer-returning math functions
                "fld" | "cld" | "div" | "mod" | "rem" => ValueType::I64,
                "length" | "size" | "ndims" => ValueType::I64,
                // Date/time accessor functions that return integers
                "year" | "month" | "day" | "hour" | "minute" | "second"
                | "dayofweek" | "dayofyear" | "week" | "days" => ValueType::I64,
                "zeros" | "ones" | "fill" | "trues" | "falses" | "collect" => ValueType::Array,
                // rand() with no args returns F64, rand(n) or rand(m,n) returns Array
                "rand" | "randn" => {
                    if args.is_empty() {
                        ValueType::F64
                    } else {
                        ValueType::Array
                    }
                }
                "println" | "print" | "sleep" | "error" | "throw" => ValueType::Nothing, // These return nothing
                // iterate returns (element, state) tuple or nothing
                // Return Any because it can be Nothing - IndexLoad handles tuples at runtime
                "iterate" => ValueType::Any,
                // abs, abs2, sign preserve argument type (Int64 → Int64, Float64 → Float64)
                // BUT: abs(Complex) returns Float64 (magnitude is always real)
                "abs" | "abs2" | "sign" => {
                    if !args.is_empty() {
                        let arg_ty = infer_value_type(&args[0], locals);
                        // Complex numbers: abs returns Float64 (magnitude)
                        if matches!(arg_ty, ValueType::Struct(_)) {
                            ValueType::F64
                        } else {
                            arg_ty
                        }
                    } else {
                        ValueType::F64
                    }
                }
                // Broadcast calls (f.(args)) return Array
                _ if function.starts_with('.') => ValueType::Array,
                // Unknown function calls (including struct constructors) - use Any
                // The actual type will be determined during compilation
                _ => ValueType::Any,
            }
        }
        Expr::Index { array, indices, .. } => {
            // Check if this is a slice operation (using Range or SliceAll)
            let is_slice = indices
                .iter()
                .any(|idx| matches!(idx, Expr::Range { .. } | Expr::SliceAll { .. }));
            let array_type = infer_value_type(array, locals);
            if array_type == ValueType::Str {
                if is_slice {
                    ValueType::Str // String slice returns String
                } else {
                    ValueType::Char // String indexing returns Char
                }
            } else if is_slice {
                array_type // Array slice preserves array type
            } else {
                // Array/Tuple element access - type determined at runtime
                ValueType::Any
            }
        }
        Expr::LetBlock { body, .. } => {
            // Let block returns the type of its last expression
            if let Some(Stmt::Expr { expr, .. }) = body.stmts.last() {
                infer_value_type(expr, locals)
            } else {
                ValueType::Nothing
            }
        }
        Expr::TupleLiteral { .. } => ValueType::Tuple,
        Expr::Pair { .. } => ValueType::Tuple, // Pair is also a tuple
        Expr::NamedTupleLiteral { .. } => ValueType::Tuple,
        // QuoteLiteral produces Expr value (AST metaprogramming)
        Expr::QuoteLiteral { .. } => ValueType::Expr,
        Expr::ModuleCall {
            module, function, ..
        } => {
            // Handle Core.Intrinsics return types
            if module == "Core.Intrinsics" {
                match function.as_str() {
                    // Float intrinsics return F64
                    "add_float" | "sub_float" | "mul_float" | "div_float" | "pow_float"
                    | "neg_float" | "abs_float" | "copysign_float" | "sqrt_llvm" | "floor_llvm"
                    | "ceil_llvm" | "trunc_llvm" | "sitofp" => ValueType::F64,
                    // Comparison intrinsics return Bool
                    "eq_int" | "ne_int" | "slt_int" | "sle_int" | "sgt_int" | "sge_int"
                    | "eq_float" | "ne_float" | "lt_float" | "le_float" | "gt_float"
                    | "ge_float" => ValueType::Bool,
                    // All other int intrinsics return I64
                    _ => ValueType::I64,
                }
            } else if module == "Base" {
                // Base.func calls - infer like regular calls
                match function.as_str() {
                    "sqrt" | "sin" | "cos" | "tan" | "exp" | "log" | "abs" => ValueType::F64,
                    "length" | "size" => ValueType::I64,
                    _ => ValueType::Any,
                }
            } else {
                ValueType::Any
            }
        }
        // Handle new{T}(...) expressions - return Any since we don't know the struct type here
        Expr::New { .. } => ValueType::Any,
        // Ternary expressions: infer type from branches
        Expr::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            let then_ty = infer_value_type(then_expr, locals);
            let else_ty = infer_value_type(else_expr, locals);
            // If both branches are the same type, return that type
            if then_ty == else_ty {
                then_ty
            } else if then_ty == ValueType::Tuple || else_ty == ValueType::Tuple {
                // If either branch is a Tuple, the result is a Tuple
                ValueType::Tuple
            } else if let Some(promoted) = promote_numeric_value_types(&then_ty, &else_ty) {
                promoted
            } else {
                ValueType::Any
            }
        }
        // Unary operations
        Expr::UnaryOp { op, operand, .. } => {
            use crate::ir::core::UnaryOp;
            match op {
                UnaryOp::Not => ValueType::Bool,        // ! always returns Bool
                _ => infer_value_type(operand, locals), // Neg, Pos preserve operand type
            }
        }
        _ => ValueType::I64, // Default fallback
    }
}

/// Infer the ValueType of an expression with struct table awareness.
/// This version properly identifies struct constructor calls.
/// This is an internal helper function used by the inference engine.
fn infer_value_type_with_structs(
    expr: &Expr,
    locals: &HashMap<String, ValueType>,
    struct_table: &HashMap<String, StructInfo>,
    global_types: &HashMap<String, ValueType>,
) -> ValueType {
    match expr {
        Expr::Call { function, args, .. } => {
            // Check if this is a struct constructor call (exact match)
            if let Some(struct_info) = struct_table.get(function) {
                return ValueType::Struct(struct_info.type_id);
            }
            // Check for parametric struct constructor like "Complex{Bool}" or "Complex{Float64}"
            if let Some(brace_idx) = function.find('{') {
                let base_name = &function[..brace_idx];
                // First check if exact match exists
                if let Some(struct_info) = struct_table.get(function) {
                    return ValueType::Struct(struct_info.type_id);
                }
                // Check if base name exists (non-parametric struct with same name)
                if let Some(struct_info) = struct_table.get(base_name) {
                    return ValueType::Struct(struct_info.type_id);
                }
                // Check if any parametric instantiation of the same base name exists
                // e.g., for "Complex{Bool}", check if "Complex{Float64}" or similar exists
                let base_prefix = format!("{}{{", base_name);
                for (name, struct_info) in struct_table {
                    if name.starts_with(&base_prefix) {
                        // Use the first matching instantiation's type_id as a proxy
                        // This tells the compiler it's a struct type
                        return ValueType::Struct(struct_info.type_id);
                    }
                }
            }
            // Math functions that preserve struct type (e.g., exp(Complex) -> Complex)
            // These functions are implemented in Pure Julia with struct overloads.
            // IMPORTANT: This list is a type inference hint, independent of whether the function
            // is a Rust builtin or Pure Julia. Do NOT remove entries during builtin migration.
            // See Issue #2634 and docs/vm/BUILTIN_REMOVAL.md Layer 5.
            // NOTE: abs is intentionally excluded because abs(Complex) returns Float64 (the magnitude)
            let struct_preserving_funcs = [
                "exp",
                "log",
                "sqrt",
                "sin",
                "cos",
                "tan",
                "asin",
                "acos",
                "atan",
                "sinh",
                "cosh",
                "tanh",
                "conj",
                "angle",
                "adjoint",
                "transpose",
            ];
            if struct_preserving_funcs.contains(&function.as_str()) && !args.is_empty() {
                let arg_ty = infer_value_type_with_structs(&args[0], locals, struct_table, global_types);
                if let ValueType::Struct(_) = arg_ty {
                    // Struct-returning functions preserve the struct type
                    return arg_ty;
                }
            }
            // Handle abs specially: abs(Complex) returns Float64 (magnitude)
            if function == "abs" && !args.is_empty() {
                let arg_ty = infer_value_type_with_structs(&args[0], locals, struct_table, global_types);
                if matches!(arg_ty, ValueType::Struct(_)) {
                    return ValueType::F64;
                }
            }
            // Check if function is a base name of a parametric struct (e.g., "Complex" -> "Complex{Float64}")
            // This handles cases like Complex(r, i) where Complex is the base parametric struct
            let prefix = format!("{}{{", function);
            for (name, struct_info) in struct_table {
                if name.starts_with(&prefix) {
                    return ValueType::Struct(struct_info.type_id);
                }
            }
            // For other calls, delegate to base function
            infer_value_type(expr, locals)
        }
        Expr::BinaryOp {
            op, left, right, ..
        } => {
            // For binary operations, recursively check with struct awareness
            let lt = infer_value_type_with_structs(left, locals, struct_table, global_types);
            let rt = infer_value_type_with_structs(right, locals, struct_table, global_types);

            // Check if either operand is a struct
            let left_struct = matches!(lt, ValueType::Struct(_));
            let right_struct = matches!(rt, ValueType::Struct(_));

            if left_struct || right_struct {
                // For arithmetic operations involving structs, preserve the struct type
                // This is critical for Complex number dispatch
                match op {
                    // Comparisons return Bool (Julia semantics)
                    BinaryOp::Lt
                    | BinaryOp::Gt
                    | BinaryOp::Le
                    | BinaryOp::Ge
                    | BinaryOp::Eq
                    | BinaryOp::Ne => ValueType::Bool,
                    // Logical operators return Bool
                    BinaryOp::And | BinaryOp::Or => ValueType::Bool,
                    // For arithmetic operations, compute proper type promotion
                    _ => {
                        // Handle Complex number type promotion per Julia semantics
                        // Extract struct names if they're Complex types
                        // Build reverse map for O(1) lookup (Issue #3358)
                        let type_id_to_name = build_type_id_to_name(struct_table);
                        let left_complex_name = if let ValueType::Struct(_) = lt {
                            get_struct_name(&lt, &type_id_to_name).filter(|n| n.starts_with("Complex{"))
                        } else {
                            None
                        };
                        let right_complex_name = if let ValueType::Struct(_) = rt {
                            get_struct_name(&rt, &type_id_to_name).filter(|n| n.starts_with("Complex{"))
                        } else {
                            None
                        };
                        // Determine the promoted Complex type based on operand types
                        // Using centralized promotion module (Julia's promote_rule/promote_type pattern)
                        match (&left_complex_name, &right_complex_name) {
                            // Both operands are Complex
                            (Some(left_name), Some(right_name)) => {
                                // Promote: Complex{Bool} + Complex{Float64} -> Complex{Float64}
                                let promoted = promote_complex(left_name, right_name);
                                find_complex_type_in_table(&promoted, struct_table).unwrap_or(lt)
                            }
                            // Left is Complex, right is primitive
                            (Some(left_name), None) => {
                                // e.g., Complex{Bool} * Float64 -> Complex{Float64}
                                let right_type_name = value_type_to_type_name(&rt);
                                let promoted = promote_complex(left_name, right_type_name);
                                find_complex_type_in_table(&promoted, struct_table).unwrap_or(lt)
                            }
                            // Left is primitive, right is Complex
                            (None, Some(right_name)) => {
                                // e.g., Float64 * Complex{Bool} -> Complex{Float64}
                                let left_type_name = value_type_to_type_name(&lt);
                                let promoted = promote_complex(left_type_name, right_name);
                                find_complex_type_in_table(&promoted, struct_table).unwrap_or(rt)
                            }
                            // Neither is Complex (shouldn't happen in this branch, but handle it)
                            (None, None) => {
                                if left_struct {
                                    lt
                                } else {
                                    rt
                                }
                            }
                        }
                    }
                }
            } else {
                // For primitive types, determine result type
                match op {
                    // Comparisons return Bool (Julia semantics)
                    BinaryOp::Lt
                    | BinaryOp::Gt
                    | BinaryOp::Le
                    | BinaryOp::Ge
                    | BinaryOp::Eq
                    | BinaryOp::Ne => ValueType::Bool,
                    // Logical operators return Bool
                    BinaryOp::And | BinaryOp::Or => ValueType::Bool,
                    // Division returns F64 for primitives
                    BinaryOp::Div => ValueType::F64,
                    // Power operator: Int^Int -> Int, Any operands -> Any (for type inference), otherwise -> Float64 (Julia semantics)
                    BinaryOp::Pow => {
                        if lt == ValueType::Any || rt == ValueType::Any {
                            ValueType::Any
                        } else if lt == ValueType::I64 && rt == ValueType::I64 {
                            ValueType::I64
                        } else {
                            ValueType::F64
                        }
                    }
                    // Mul with strings is string concatenation
                    BinaryOp::Mul => {
                        if lt == ValueType::Str || rt == ValueType::Str {
                            ValueType::Str
                        } else if let Some(promoted) = promote_numeric_value_types(&lt, &rt) {
                            promoted
                        } else if lt == ValueType::Any || rt == ValueType::Any {
                            ValueType::Any
                        } else if lt == ValueType::F64 || rt == ValueType::F64 {
                            ValueType::F64
                        } else {
                            ValueType::I64
                        }
                    }
                    // Other arithmetic operations follow type promotion
                    _ => {
                        if let Some(promoted) = promote_numeric_value_types(&lt, &rt) {
                            promoted
                        } else if lt == ValueType::Any || rt == ValueType::Any {
                            ValueType::Any
                        } else if lt == ValueType::F64 || rt == ValueType::F64 {
                            ValueType::F64
                        } else {
                            ValueType::I64
                        }
                    }
                }
            }
        }
        Expr::Var(name, _) => {
            // Check locals first, then global_types (e.g., prelude consts like `im`)
            if let Some(ty) = locals.get(name).or_else(|| global_types.get(name)) {
                ty.clone()
            } else {
                infer_value_type(expr, locals)
            }
        }
        Expr::FieldAccess { object, field, .. } => {
            // Infer the type of the object and look up the field type
            let obj_type = infer_value_type_with_structs(object, locals, struct_table, global_types);
            if let ValueType::Struct(type_id) = obj_type {
                // Use reverse map for O(1) lookup instead of O(n) scan (Issue #3358)
                let type_id_to_name = build_type_id_to_name(struct_table);
                if let Some(struct_name) = type_id_to_name.get(&type_id) {
                    if let Some(struct_info) = struct_table.get(struct_name) {
                        for (field_name, field_ty) in &struct_info.fields {
                            if field_name == field {
                                return field_ty.clone();
                            }
                        }
                    }
                }
            }
            // Handle Expr type field access: head -> Symbol, args -> Array
            if obj_type == ValueType::Expr {
                match field.as_str() {
                    "head" => return ValueType::Symbol,
                    "args" => return ValueType::Array,
                    _ => {}
                }
            }
            // For Any type objects or unknown fields, return Any to allow runtime type determination
            // This is important for where clause methods like real(z::Complex{T}) where T<:Real
            ValueType::Any
        }
        // Handle struct literals directly (e.g., `im` which lowers to Literal::Struct("Complex{Float64}", ...))
        Expr::Literal(Literal::Struct(struct_name, _), _) => {
            // Look up the struct in the struct table
            if let Some(struct_info) = struct_table.get(struct_name) {
                return ValueType::Struct(struct_info.type_id);
            }
            // Check for parametric struct base name (e.g., "Complex{Float64}" -> check "Complex" too)
            if let Some(brace_idx) = struct_name.find('{') {
                let base_name = &struct_name[..brace_idx];
                // Look for any instantiation of this parametric struct
                let prefix = format!("{}{{", base_name);
                for (name, struct_info) in struct_table {
                    if name.starts_with(&prefix) || name == struct_name {
                        return ValueType::Struct(struct_info.type_id);
                    }
                }
            }
            ValueType::Any
        }
        // Handle new{T}(...) expressions used in inner constructors
        // These create struct instances, so return Any to indicate unknown struct type
        // The actual struct type will be determined from the constructor function's context
        Expr::New { .. } => ValueType::Any,
        // Ternary expressions: infer type from branches with struct awareness
        Expr::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            let then_ty = infer_value_type_with_structs(then_expr, locals, struct_table, global_types);
            let else_ty = infer_value_type_with_structs(else_expr, locals, struct_table, global_types);
            // If both branches are the same type, return that type
            if then_ty == else_ty {
                then_ty
            } else if then_ty == ValueType::Tuple || else_ty == ValueType::Tuple {
                // If either branch is a Tuple, the result is a Tuple
                ValueType::Tuple
            } else if matches!(then_ty, ValueType::Struct(_)) {
                // If then branch is a struct, prefer that type
                then_ty
            } else if matches!(else_ty, ValueType::Struct(_)) {
                // If else branch is a struct, use that type
                else_ty
            } else if let Some(promoted) = promote_numeric_value_types(&then_ty, &else_ty) {
                promoted
            } else {
                ValueType::Any
            }
        }
        // Typed empty array like Bool[], Int64[], Float64[]
        Expr::TypedEmptyArray { element_type, .. } => {
            match element_type.as_str() {
                "Int" | "Int64" => ValueType::ArrayOf(ArrayElementType::I64),
                "Int32" => ValueType::ArrayOf(ArrayElementType::I64),
                "Float64" | "Float32" => ValueType::ArrayOf(ArrayElementType::F64),
                "Bool" => ValueType::ArrayOf(ArrayElementType::Bool),
                "String" => ValueType::ArrayOf(ArrayElementType::String),
                "Char" => ValueType::ArrayOf(ArrayElementType::Char),
                type_name => {
                    // Check if it's a struct type
                    let base_name = type_name.split('{').next().unwrap_or(type_name);
                    if let Some(struct_info) = struct_table.get(base_name) {
                        ValueType::ArrayOf(ArrayElementType::StructOf(struct_info.type_id))
                    } else {
                        ValueType::ArrayOf(ArrayElementType::Any)
                    }
                }
            }
        }
        // Note: Adjoint and Transpose have been migrated to Pure Julia
        // They are no longer builtins and use method dispatch instead
        Expr::Builtin { .. } => infer_value_type(expr, locals),
        // For all other expression types, delegate to the base function
        _ => infer_value_type(expr, locals),
    }
}

/// Infer the return type of a function using explicit argument ValueTypes.
///
/// This enables call-site specialization when parameter annotations are absent.
pub fn infer_function_return_type_v2_with_arg_types(
    func: &Function,
    struct_table: &HashMap<String, StructInfo>,
    arg_value_types: &[ValueType],
) -> ValueType {
    use crate::compile::abstract_interp::{InferenceEngine, StructTypeInfo};
    use crate::compile::bridge::{lattice_to_value_type, value_type_to_lattice_with_struct_table};
    use crate::compile::lattice::types::LatticeType;
    use std::collections::HashMap as StdHashMap;

    let lattice_struct_table: StdHashMap<String, StructTypeInfo> = struct_table
        .iter()
        .map(|(name, info)| {
            let fields_map: StdHashMap<String, LatticeType> = info
                .fields
                .iter()
                .map(|(fname, ftype)| {
                    let lattice_type = value_type_to_lattice_with_struct_table(ftype, struct_table);
                    (fname.clone(), lattice_type)
                })
                .collect();

            (
                name.clone(),
                StructTypeInfo {
                    type_id: info.type_id,
                    is_mutable: info.is_mutable,
                    fields: fields_map,
                    has_inner_constructor: info.has_inner_constructor,
                },
            )
        })
        .collect();

    let arg_lattice_types: Vec<LatticeType> = arg_value_types
        .iter()
        .map(|vt| value_type_to_lattice_with_struct_table(vt, struct_table))
        .collect();

    let mut engine = InferenceEngine::with_struct_table(lattice_struct_table);
    let return_type = engine.infer_function_with_arg_types(func, &arg_lattice_types);

    lattice_to_value_type(&return_type)
}

/// Build a shared `InferenceEngine` pre-populated with the struct table and all
/// program functions. Creating the engine once and reusing it across the entire
/// function compilation loop avoids O(n^2) function cloning and struct-table
/// conversion that occurred when `infer_function_return_type_v2_with_functions`
/// was called inside the loop.
///
/// The returned engine can be used with `engine.infer_function(func)` and its
/// return-type cache is shared across calls, further reducing redundant work.
pub fn build_shared_inference_engine<'a>(
    struct_table: &HashMap<String, StructInfo>,
    all_functions: impl IntoIterator<Item = &'a Function>,
) -> crate::compile::abstract_interp::InferenceEngine {
    use crate::compile::abstract_interp::{InferenceEngine, StructTypeInfo};
    use crate::compile::bridge::value_type_to_lattice_with_struct_table;
    use crate::compile::lattice::types::LatticeType;
    use std::collections::HashMap as StdHashMap;

    // Convert StructInfo to StructTypeInfo (done once)
    let lattice_struct_table: StdHashMap<String, StructTypeInfo> = struct_table
        .iter()
        .map(|(name, info)| {
            let fields_map: StdHashMap<String, LatticeType> = info
                .fields
                .iter()
                .map(|(fname, ftype)| {
                    let lattice_type = value_type_to_lattice_with_struct_table(ftype, struct_table);
                    (fname.clone(), lattice_type)
                })
                .collect();

            (
                name.clone(),
                StructTypeInfo {
                    type_id: info.type_id,
                    is_mutable: info.is_mutable,
                    fields: fields_map,
                    has_inner_constructor: info.has_inner_constructor,
                },
            )
        })
        .collect();

    // Create engine and clone all functions once
    let mut engine = InferenceEngine::with_struct_table(lattice_struct_table);
    for f in all_functions {
        engine.add_function(f.clone());
    }

    engine
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::context::StructInfo;
    use crate::ir::core::{Expr, Literal, Stmt};
    use crate::span::Span;
    use crate::vm::ValueType;

    fn span() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    fn int_lit(v: i64) -> Expr {
        Expr::Literal(Literal::Int(v), span())
    }

    fn float_lit(v: f64) -> Expr {
        Expr::Literal(Literal::Float(v), span())
    }

    fn var_expr(name: &str) -> Expr {
        Expr::Var(name.to_string(), span())
    }

    fn assign_stmt(var: &str, value: Expr) -> Stmt {
        Stmt::Assign {
            var: var.to_string(),
            value,
            span: span(),
        }
    }

    // ── promote_numeric_value_types ──────────────────────────────────────────

    #[test]
    fn test_promote_numeric_same_type() {
        assert_eq!(
            promote_numeric_value_types(&ValueType::I64, &ValueType::I64),
            Some(ValueType::I64)
        );
        assert_eq!(
            promote_numeric_value_types(&ValueType::F64, &ValueType::F64),
            Some(ValueType::F64)
        );
    }

    #[test]
    fn test_promote_numeric_int_float_to_float() {
        assert_eq!(
            promote_numeric_value_types(&ValueType::I64, &ValueType::F64),
            Some(ValueType::F64)
        );
        assert_eq!(
            promote_numeric_value_types(&ValueType::F64, &ValueType::I64),
            Some(ValueType::F64)
        );
    }

    #[test]
    fn test_promote_numeric_any_returns_none() {
        // Any is not a concrete numeric type
        assert_eq!(promote_numeric_value_types(&ValueType::Any, &ValueType::I64), None);
        assert_eq!(promote_numeric_value_types(&ValueType::I64, &ValueType::Any), None);
    }

    #[test]
    fn test_promote_numeric_non_numeric_returns_none() {
        assert_eq!(
            promote_numeric_value_types(&ValueType::Str, &ValueType::I64),
            None
        );
        assert_eq!(
            promote_numeric_value_types(&ValueType::I64, &ValueType::Str),
            None
        );
    }

    // ── collect_global_types_for_inference ───────────────────────────────────

    #[test]
    fn test_collect_global_types_int_literal() {
        let stmts = vec![assign_stmt("x", int_lit(42))];
        let mut globals = HashMap::new();
        let struct_table = HashMap::new();
        let mut const_structs = HashMap::new();
        collect_global_types_for_inference(&stmts, &mut globals, &struct_table, &mut const_structs);
        assert_eq!(globals.get("x"), Some(&ValueType::I64));
    }

    #[test]
    fn test_collect_global_types_float_literal() {
        let stmts = vec![assign_stmt("y", float_lit(1.25))];
        let mut globals = HashMap::new();
        let struct_table = HashMap::new();
        let mut const_structs = HashMap::new();
        collect_global_types_for_inference(&stmts, &mut globals, &struct_table, &mut const_structs);
        assert_eq!(globals.get("y"), Some(&ValueType::F64));
    }

    #[test]
    fn test_collect_global_types_reference_previously_defined() {
        // `b = a` where `a = 42` was defined before should pick up a's type
        let stmts = vec![
            assign_stmt("a", int_lit(42)),
            assign_stmt("b", var_expr("a")),
        ];
        let mut globals = HashMap::new();
        let struct_table = HashMap::new();
        let mut const_structs = HashMap::new();
        collect_global_types_for_inference(&stmts, &mut globals, &struct_table, &mut const_structs);
        assert_eq!(globals.get("a"), Some(&ValueType::I64));
        assert_eq!(globals.get("b"), Some(&ValueType::I64));
    }

    #[test]
    fn test_collect_global_types_struct_constructor() {
        // `m = MyType()` → globals["m"] = Struct(type_id)
        let mut struct_table = HashMap::new();
        struct_table.insert(
            "MyType".to_string(),
            StructInfo {
                type_id: 5,
                is_mutable: false,
                fields: vec![],
                has_inner_constructor: false,
            },
        );
        let call_expr = Expr::Call {
            function: "MyType".to_string(),
            args: vec![],
            kwargs: vec![],
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span: span(),
        };
        let stmts = vec![assign_stmt("m", call_expr)];
        let mut globals = HashMap::new();
        let mut const_structs = HashMap::new();
        collect_global_types_for_inference(&stmts, &mut globals, &struct_table, &mut const_structs);
        assert_eq!(globals.get("m"), Some(&ValueType::Struct(5)));
        // Empty-arg struct → also tracked in const_structs
        assert!(const_structs.contains_key("m"));
    }

    // ── infer_value_type_with_structs: global_types propagation (Issue #3088) ──

    #[test]
    fn test_infer_var_with_global_types_fallback() {
        // When a Var is not in locals, it should be found in global_types
        let expr = var_expr("im_const");
        let locals = HashMap::new();
        let struct_table = HashMap::new();
        let mut global_types = HashMap::new();
        global_types.insert("im_const".to_string(), ValueType::Struct(3));

        let result = infer_value_type_with_structs(&expr, &locals, &struct_table, &global_types);
        assert_eq!(result, ValueType::Struct(3));
    }

    #[test]
    fn test_infer_var_locals_take_priority_over_globals() {
        // locals takes precedence over global_types
        let expr = var_expr("x");
        let mut locals = HashMap::new();
        locals.insert("x".to_string(), ValueType::F64);
        let struct_table = HashMap::new();
        let mut global_types = HashMap::new();
        global_types.insert("x".to_string(), ValueType::I64); // different in globals

        let result = infer_value_type_with_structs(&expr, &locals, &struct_table, &global_types);
        assert_eq!(result, ValueType::F64); // locals wins
    }

    #[test]
    fn test_infer_var_unknown_returns_any() {
        let expr = var_expr("totally_unknown");
        let locals = HashMap::new();
        let struct_table = HashMap::new();
        let global_types = HashMap::new();
        let result = infer_value_type_with_structs(&expr, &locals, &struct_table, &global_types);
        assert_eq!(result, ValueType::Any);
    }
}

