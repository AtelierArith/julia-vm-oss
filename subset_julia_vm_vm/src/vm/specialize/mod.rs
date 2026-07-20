//! Runtime specialization engine for Lazy AoT compilation.
//!
//! This module provides the lightweight compiler that runs at first call
//! to generate specialized bytecode based on actual argument types (typeof(x)).
//!
//! ## Supported Statement Types
//!
//! The following statement types can be specialized:
//! - `Block` - Sequential statement blocks
//! - `Assign` - Variable assignment
//! - `AddAssign` - Addition assignment (+=)
//! - `IndexAssign` - 1D Int64/Float64 array element assignment
//! - `FieldAssign` - Mutable-struct field assignment with a statically resolved
//!   field index (`obj.field = value`); the value is coerced to the field type
//!   to match the interpreter (Issue #6346)
//! - `For` - Numeric for loops
//! - `ForEach` - Iteration loops (for x in iter)
//! - `While` - While loops
//! - `If` - Conditional branches
//! - `Break` / `Continue` - Loop control
//! - `Return` - Function return
//! - `Expr` - Expression statements
//!
//! ## Unsupported Statement Types (falls back to interpreter)
//!
//! - `Try` - Exception handling (requires complex control flow)
//! - `DestructuringAssign` - Tuple/array destructuring. Note: the lowering pass
//!   desugars `a, b = ...` into a temporary tuple plus indexed `Assign`s before
//!   the specializer runs, so this IR variant is not produced by the current
//!   pipeline. The *desugared* swap (`a, b = b, a % b`) is kept type-stable by
//!   tracking the tuple-literal temporary's element types and sharpening the
//!   constant `temp[i]` reads (Issue #6561 — see `tuple_element_types` and
//!   `try_compile_tracked_tuple_index`).
//! - `DictAssign` - Dictionary assignment
//! - `FunctionDef` - Nested function definitions
//! - `Test*` - Test framework macros
//! - `Label` / `Goto` - Jump labels (rarely used)
//!
//! ## Supported Expression Types
//!
//! - `Literal` - Int, Float, Bool, String, Nothing, Missing
//! - `Var` - Variable references
//! - `BinaryOp` - Binary operations (+, -, *, /, etc.)
//! - `UnaryOp` - Unary operations (-, !, etc.)
//! - `Call` - Function calls (including operators as functions)
//! - `Builtin` - Builtin function calls
//! - `ArrayLiteral` - Array construction
//! - `Index` - Array/tuple indexing
//! - `FieldAccess` - Struct field read `obj.field` on a known struct type, with
//!   the field index/type resolved statically (Issue #6346)
//! - `TupleLiteral` - Tuple construction
//! - `Range` - Range expressions (start:stop, start:step:stop)
//!
//! ## Error Messages
//!
//! When specialization fails for an unsupported construct, the error message
//! includes the readable variant name (e.g., "IndexAssign", "Try") rather than
//! an opaque discriminant. This is ensured by `stmt_variant_name()` and
//! `expr_variant_name()` which use exhaustive pattern matching - adding new
//! enum variants will cause a compiler error if these functions aren't updated.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::ir::core::{Expr, Function, Literal};
use crate::vm::{AbstractTypeDefInfo, Instr, RuntimeCompileContext, StructDefInfo, ValueType};

mod expr;
mod helpers;
#[cfg(test)]
mod issue_10970_tests;
#[cfg(test)]
mod issue_10970_regression {
    #[test]
    fn compile_index_with_lossy_struct_index_widens_result_issue_10970() -> Result<(), String> {
        super::issue_10970_tests::compile_index_with_lossy_struct_index_widens_result_issue_10970()
    }

    #[test]
    fn compile_typed_array_literal_emits_literal_build_issue_10746() -> Result<(), String> {
        super::issue_10970_tests::compile_typed_array_literal_emits_literal_build_issue_10746()
    }
}
mod stmt;

#[cfg(test)]
use helpers::{expr_variant_name, stmt_variant_name};

/// A user-defined function callable from within another function's runtime
/// specialization (Issue #10749). `compile_call` consults a table of these
/// (keyed by bare function name) to recognize a call to ANOTHER
/// specializable user function and emit `Instr::CallSpecialize` for it
/// directly, instead of failing the whole caller's specialization the moment
/// it sees a callee it doesn't recognize.
///
/// Built once per VM call-table generation, restricted to names that resolve
/// to exactly one method anywhere in the function table: Julia multiple
/// dispatch on an ambiguous bare name cannot be soundly resolved at this
/// layer (the specializer has no argument-type-driven method resolution), so
/// such names are simply excluded — calls to them keep falling back to the
/// pre-existing `Unsupported` path, same as before this issue.
#[derive(Clone)]
pub struct SpecializableCallee {
    /// Index into `Vm::specializable_functions` / the operand expected by
    /// `Instr::CallSpecialize`.
    pub spec_func_index: usize,
    /// The callee's own Core IR, reused (recursively, bounded) to infer its
    /// return type for this exact call site's concrete argument types.
    pub ir: Arc<Function>,
    /// Declared positional parameter count (excludes kwparams).
    pub param_count: usize,
    /// True when the last positional parameter collects `args...`.
    pub has_vararg: bool,
    /// Dotted module path used to resolve module-private type objects while
    /// recompiling the callee's body (mirrors `module_path_from_function_name`).
    pub module_path: Option<String>,
}

/// Name -> callee lookup consulted by `compile_call` (Issue #10749).
pub type CallableRegistry = HashMap<String, SpecializableCallee>;

/// Maximum nested return-type-inference depth (Issue #10749). A call chain
/// longer than this falls back to `Unsupported` rather than continuing to
/// recurse; direct/mutual recursion is caught immediately (regardless of this
/// limit) via `in_progress`.
const MAX_NESTED_SPECIALIZATION_DEPTH: usize = 6;

/// Bounds nested runtime-specialization attempts triggered when
/// `compile_call` looks up a callee's return type: a direct or mutual
/// recursive cycle, or an arbitrarily deep call chain, must not hang or
/// stack-overflow the specializer (Issue #10749). One instance is shared
/// (via `&RefCell`) across one top-level `specialize_function` call and every
/// nested call it triggers while compiling that function's body.
#[derive(Default)]
pub struct SpecializationRecursionGuard {
    in_progress: HashSet<usize>,
    depth: usize,
}

impl SpecializationRecursionGuard {
    pub fn new() -> Self {
        Self::default()
    }
}

/// RAII scope that removes a `spec_func_index` from the in-progress set (and
/// decrements the depth counter) when a `specialize_function` call returns —
/// including via an early `?`-propagated error, since `Drop` still runs on
/// unwinding through the `?` operator's ordinary return path.
struct RecursionScope<'a> {
    guard: &'a RefCell<SpecializationRecursionGuard>,
    idx: usize,
}

impl Drop for RecursionScope<'_> {
    fn drop(&mut self) {
        let mut guard = self.guard.borrow_mut();
        guard.in_progress.remove(&self.idx);
        guard.depth = guard.depth.saturating_sub(1);
    }
}

/// Error during runtime specialization
#[derive(Debug, Clone)]
pub enum SpecializationError {
    /// Type mismatch between expected and actual
    TypeMismatch {
        expected: ValueType,
        actual: ValueType,
    },
    /// Compilation failed
    CompileFailed(String),
    /// Missing compile context
    MissingContext,
    /// Unsupported expression for specialization
    Unsupported(String),
}

impl std::fmt::Display for SpecializationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpecializationError::TypeMismatch { expected, actual } => write!(
                f,
                "Type mismatch: expected {:?}, got {:?}",
                expected, actual
            ),
            SpecializationError::CompileFailed(msg) => {
                write!(f, "Specialization compile failed: {}", msg)
            }
            SpecializationError::MissingContext => write!(f, "Missing runtime compile context"),
            SpecializationError::Unsupported(msg) => write!(f, "Unsupported: {}", msg),
        }
    }
}

/// Result of specialization
#[derive(Debug)]
pub struct SpecializationResult {
    /// Generated bytecode
    pub code: Vec<Instr>,
    /// Inferred return type. NOTE: last-write-wins across return sites — only
    /// trust it as "the" return type when `return_type_consistent` is true.
    pub return_type: ValueType,
    /// True when EVERY return site of the body agreed on `return_type`
    /// (Issue #10749). A cross-function call site only propagates the callee's
    /// return type into its own typing when this holds; otherwise it returns
    /// `Unsupported` and the call falls back to the generic path.
    pub return_type_consistent: bool,
}

/// Specialize a function for specific argument types.
///
/// This is called at runtime when a function is first called with specific types.
/// The function is recompiled with those types fixed.
/// Specialize a function with NO cross-function call support: calls to other
/// user-defined functions are `Unsupported` (the pre-Issue-#10749 behavior).
///
/// Kept as the simple public entry point for callers that have no VM callee
/// registry at hand (unit/integration tests, ad-hoc analyses). Production VM
/// paths call [`specialize_function_with_callees`].
pub fn specialize_function(
    func: &Function,
    arg_types: &[ValueType],
    struct_defs: &[StructDefInfo],
    type_object_names: &HashSet<String>,
    module_path: Option<&str>,
    disable_array_index_fast_path: bool,
    disable_field_access: bool,
) -> Result<SpecializationResult, SpecializationError> {
    let registry = CallableRegistry::new();
    let guard = RefCell::new(SpecializationRecursionGuard::new());
    specialize_function_with_callees(
        func,
        arg_types,
        struct_defs,
        type_object_names,
        module_path,
        disable_array_index_fast_path,
        disable_field_access,
        &registry,
        &guard,
        None,
    )
}

/// Specialize a function, allowing its body to compile calls to OTHER
/// user-defined functions found in `callable_registry` (Issue #10749).
///
/// `own_spec_func_index` identifies the function being specialized so the
/// shared `recursion_guard` can reject a direct or mutual recursive cycle
/// before it recurses.
#[allow(clippy::too_many_arguments)]
pub fn specialize_function_with_callees(
    func: &Function,
    arg_types: &[ValueType],
    struct_defs: &[StructDefInfo],
    type_object_names: &HashSet<String>,
    module_path: Option<&str>,
    disable_array_index_fast_path: bool,
    disable_field_access: bool,
    callable_registry: &CallableRegistry,
    recursion_guard: &RefCell<SpecializationRecursionGuard>,
    own_spec_func_index: Option<usize>,
) -> Result<SpecializationResult, SpecializationError> {
    // Issue #10749: bound nested specialization triggered by a call to
    // another user-defined function inside this body (`compile_call` learns
    // the callee's return type via a nested specialization call). A
    // function already "in progress" higher up the call chain (direct or
    // mutual recursion) or a chain deeper than the limit bails out here,
    // BEFORE compiling anything, so the caller's `compile_call` sees a clean
    // `Unsupported` and falls back rather than looping or overflowing the
    // (Rust-native) call stack.
    let _recursion_scope = match own_spec_func_index {
        Some(idx) => {
            let mut guard = recursion_guard.borrow_mut();
            if guard.in_progress.contains(&idx) || guard.depth >= MAX_NESTED_SPECIALIZATION_DEPTH {
                return Err(SpecializationError::Unsupported(
                    "recursive or too-deep call chain not specialized (Issue #10749)".to_string(),
                ));
            }
            guard.in_progress.insert(idx);
            guard.depth += 1;
            drop(guard);
            Some(RecursionScope {
                guard: recursion_guard,
                idx,
            })
        }
        None => None,
    };

    // 1. Build locals map with fixed argument types
    let mut locals: HashMap<String, ValueType> = HashMap::new();
    for (param, ty) in func.params.iter().zip(arg_types.iter()) {
        let local_ty = if param.is_varargs {
            // Issue #4344: runtime calls pack varargs into a Tuple slot.
            // Specialization must mirror that calling convention instead of
            // binding the collector to the first concrete argument type.
            ValueType::Tuple
        } else {
            ty.clone()
        };
        locals.insert(param.name.clone(), local_ty);
    }

    // Where-clause type parameters are installed per call by
    // `bind_type_params` (as frame `type_bindings` DataType entries, or as
    // value locals for `Val{N}`/`NTuple{N,…}`-style integer/symbol
    // parameters) and lexically shadow any same-named builtin/global type
    // over the method body (Issue #10407). Register them as `Any` locals so
    // `compile_var` emits a dynamic `LoadAny` (which consults the frame's
    // type bindings at runtime) and `compile_call` takes the local-callee
    // path (Issue #10146) instead of baking in the builtin type object or
    // constructor — `Float64(2)` under `where {Float64}` must call the
    // TypeVar's bound type, not the builtin `Float64`. Parameter names take
    // precedence (`entry` keeps the typed param slot when both collide).
    for tp in &func.type_params {
        locals.entry(tp.name.clone()).or_insert(ValueType::Any);
    }

    // 2. Create specializer
    // 2. Identify ComplexF64 parameters that need split-slot hoisting.
    let complex_params: Vec<String> = func
        .params
        .iter()
        .zip(arg_types.iter())
        .filter_map(|(param, ty)| {
            if param.is_varargs {
                return None;
            }
            if *ty == ValueType::ComplexF64 {
                Some(param.name.clone())
            } else {
                None
            }
        })
        .collect();

    // 3. Create specializer
    let mut specializer = FunctionSpecializer::new(
        locals,
        struct_defs,
        type_object_names,
        module_path,
        callable_registry,
        recursion_guard,
    );
    specializer.disable_array_index_fast_path = disable_array_index_fast_path;
    specializer.disable_field_access = disable_field_access;

    // 4. Hoist ComplexF64 parameters into split (re, im) F64 slots.
    //    The boxed parameter is loaded, field 0 (real) and field 1 (imag) are
    //    extracted, and each is stored into a dedicated F64 slot. This preamble
    //    runs before any other function body code, including kwparam defaults.
    for param in &complex_params {
        let (re, im) = specializer.ensure_complex_split(param);
        specializer.emit(Instr::LoadAny(param.clone()));
        specializer.emit(Instr::GetField(0));
        specializer.emit(Instr::StoreF64(re));
        specializer.emit(Instr::LoadAny(param.clone()));
        specializer.emit(Instr::GetField(1));
        specializer.emit(Instr::StoreF64(im));
    }

    // 5. Compile keyword parameter defaults and bind them as locals
    // Skip required kwargs (they don't have a valid default - marked with Literal::Undef)
    for kwparam in &func.kwparams {
        // Check if the kwparam is required (default is Undef)
        let is_required = matches!(&kwparam.default, Expr::Literal(Literal::Undef, _));
        if !is_required {
            let ty = specializer.compile_expr(&kwparam.default)?;
            specializer.locals.insert(kwparam.name.clone(), ty.clone());
            specializer.emit_store(&kwparam.name, ty);
        } else {
            // Required kwparam - use Any type (actual type determined at call site)
            specializer
                .locals
                .insert(kwparam.name.clone(), ValueType::Any);
        }
    }

    // 6. Compile the function body with implicit return handling
    //    In Julia, the last expression in a function is its return value.
    //    If the last statement is an if statement, each branch should return its value.
    specializer.compile_function_body(&func.body)?;

    let return_type = specializer.current_return_type.clone();
    let return_type_consistent = !specializer.return_type_conflict;

    Ok(SpecializationResult {
        code: specializer.code,
        return_type,
        return_type_consistent,
    })
}

/// Lightweight compiler for function specialization
struct FunctionSpecializer<'a> {
    code: Vec<Instr>,
    locals: HashMap<String, ValueType>,
    current_return_type: ValueType,
    /// Whether any return-value type has been observed yet (Issue #10749).
    return_type_seen: bool,
    /// Set when two return sites of this body reported DIFFERENT types, so
    /// `current_return_type` (last-write-wins) does not describe every return.
    /// A cross-function caller must not trust the reported type in that case
    /// — see `record_return_type`.
    return_type_conflict: bool,
    /// Positions of break jumps to be patched
    break_positions: Vec<usize>,
    /// Positions of continue jumps to be patched
    continue_positions: Vec<usize>,
    /// Struct type definitions indexed by `type_id`, used to statically resolve
    /// field indices and field types for `FieldAccess` reads and `FieldAssign`
    /// writes on `ValueType::Struct(type_id)` operands (Issue #6346). Borrowed
    /// from the VM's `struct_defs` for the duration of one specialization.
    struct_defs: &'a [StructDefInfo],
    /// Dotted module path for the function being specialized. The generic
    /// compiler resolves unqualified module-private type objects through this
    /// path; lazy runtime specialization must do the same or it can turn a
    /// valid method body `T` reference into `UndefVarError: T` (Issue #8410).
    current_module_path: Option<String>,
    /// Per-element specialized types of tuple-literal temporaries (Issue #6561).
    ///
    /// The lowering pass desugars a self-referential destructuring swap such as
    /// `a, b = b, a % b` into `__tuple_tmp = (b, a % b); a = __tuple_tmp[1];
    /// b = __tuple_tmp[2]`. When the RHS is a tuple literal we record each
    /// element's specialized type here so a later constant index `temp[k]`
    /// (`compile_index`) can return the precise element type and emit a typed
    /// `Store*` instead of widening the target to `Any`.
    tuple_element_types: HashMap<String, Vec<ValueType>>,
    /// Type-object names visible to the runtime specializer. The main compiler
    /// resolves a bare module-private type in a method body through
    /// `current_module_path`; the specializer recompiles from IR later and must
    /// preserve that same DataType binding instead of emitting `LoadAny("T")`
    /// (Issue #8410).
    type_object_names: &'a HashSet<String>,
    module_path: Option<&'a str>,
    /// Issue #6657: when the program defines a user `getindex` override on a
    /// native array receiver, the specializer must NOT emit its native-indexing
    /// fast path (`IndexLoad`) for a scalar `xs[i]` — that would bypass the
    /// override. With this set, `compile_index` bails out (`Unsupported`) so the
    /// whole specialization is abandoned and the generic body (whose
    /// `CallTypedDispatchOrBuiltin(GetIndex, ..)` reaches the override at
    /// runtime) is used instead. Default `false` keeps the hot path intact.
    disable_array_index_fast_path: bool,
    /// Issue #8127: when the program defines a user `getproperty` override, the
    /// specializer must NOT emit a direct `GetField` for an `obj.field` read —
    /// that would bypass the override. With this set, `compile_field_access`
    /// bails out (`Unsupported`) so the specialization is abandoned and the
    /// generic body (whose interpreter `getproperty` dispatch reaches the
    /// override) is used instead. Default `false` keeps the hot path intact.
    disable_field_access: bool,
    /// Issue #10567: maps a `ComplexF64` local name to the generated `(re, im)`
    /// `F64` slot names used by the split-slot SROA fast path.
    complex_splits: HashMap<String, (String, String)>,
    /// Issue #10749: name -> callee lookup so `compile_call` can recognize a
    /// call to another user-defined (specializable) function.
    callable_registry: &'a CallableRegistry,
    /// Issue #10749: shared recursion/depth guard for nested
    /// `specialize_function` calls triggered while resolving a callee's
    /// return type from `compile_call`.
    recursion_guard: &'a RefCell<SpecializationRecursionGuard>,
}

pub(crate) fn collect_type_object_names(
    struct_defs: &[StructDefInfo],
    compile_context: Option<&RuntimeCompileContext>,
    abstract_types: &[AbstractTypeDefInfo],
) -> HashSet<String> {
    let mut names: HashSet<String> = struct_defs.iter().map(|def| def.name.clone()).collect();
    names.extend(abstract_types.iter().map(|def| def.name.clone()));
    if let Some(ctx) = compile_context {
        names.extend(ctx.parametric_structs.keys().cloned());
        names.extend(ctx.primitive_types.iter().map(|def| def.name.clone()));
        names.extend(ctx.type_aliases.keys().cloned());
    }
    names
}

pub(crate) fn module_path_from_function_name(name: &str) -> Option<&str> {
    name.rsplit_once('.')
        .map(|(module, _)| module)
        .filter(|module| !module.is_empty())
}

impl<'a> FunctionSpecializer<'a> {
    /// Generate the split-slot names for the real and imaginary parts of a
    /// `ComplexF64` local named `name`.
    fn cx_slot_names(&self, name: &str) -> (String, String) {
        (
            format!("__sjulia_cx_re_{}", name),
            format!("__sjulia_cx_im_{}", name),
        )
    }

    /// Ensure a split-slot entry exists for `name`, creating it if necessary.
    fn ensure_complex_split(&mut self, name: &str) -> (String, String) {
        let names = self.cx_slot_names(name);
        self.complex_splits
            .entry(name.to_string())
            .or_insert(names)
            .clone()
    }

    /// Returns `true` if `name` has been registered as a split `ComplexF64` local.
    ///
    /// Currently only used by tests and by the upcoming expr/stmt SROA tasks
    /// (Issue #10567); `#[allow(dead_code)]` keeps the lib build warning-free
    /// until those consumers land.
    #[allow(dead_code)]
    fn is_complex_split(&self, name: &str) -> bool {
        self.complex_splits.contains_key(name)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::ir::core::{BinaryOp, Block, Expr, Function, Literal, Stmt, TypedParam};
    use crate::span::Span;
    use subset_julia_vm_bytecode::ArrayElementType;

    /// Helper to create a test span
    fn test_span() -> Span {
        Span::new(0, 0, 1, 1, 1, 1)
    }

    /// Verify that stmt_variant_name returns readable names for all statement types.
    /// This test ensures error messages are human-readable, not opaque discriminants.
    /// (Issue #2210, #2229)
    #[test]
    fn test_stmt_variant_name_returns_readable_names() {
        let span = test_span();

        // Test a representative sample of statement types
        let test_cases = vec![
            (
                Stmt::Block(Block {
                    stmts: vec![],
                    span,
                }),
                "Block",
            ),
            (
                Stmt::Assign {
                    var: "x".to_string(),
                    value: Expr::Literal(Literal::Int(1), span),
                    span,
                },
                "Assign",
            ),
            (
                Stmt::Try {
                    try_block: Block {
                        stmts: vec![],
                        span,
                    },
                    catch_var: Some("e".to_string()),
                    catch_block: Some(Block {
                        stmts: vec![],
                        span,
                    }),
                    else_block: None,
                    finally_block: None,
                    span,
                },
                "Try",
            ),
            (
                Stmt::IndexAssign {
                    array: "arr".to_string(),
                    indices: vec![],
                    value: Expr::Literal(Literal::Int(1), span),
                    span,
                },
                "IndexAssign",
            ),
            (
                Stmt::FieldAssign {
                    object: "obj".to_string(),
                    field: "x".to_string(),
                    value: Expr::Literal(Literal::Int(1), span),
                    span,
                },
                "FieldAssign",
            ),
        ];

        for (stmt, expected_name) in test_cases {
            let name = stmt_variant_name(&stmt);
            assert_eq!(
                name, expected_name,
                "stmt_variant_name should return '{}' for {:?}",
                expected_name, stmt
            );
            // Verify the name doesn't look like a discriminant
            assert!(
                !name.starts_with("Variant("),
                "stmt_variant_name should not return discriminant format: {}",
                name
            );
        }
    }

    /// Verify that expr_variant_name returns readable names for all expression types.
    /// (Issue #2210, #2229)
    #[test]
    fn test_expr_variant_name_returns_readable_names() {
        let span = test_span();

        let test_cases = vec![
            (Expr::Literal(Literal::Int(42), span), "Literal"),
            (Expr::Var("x".to_string().into(), span), "Var"),
            (
                Expr::BinaryOp {
                    op: BinaryOp::Add,
                    left: Box::new(Expr::Literal(Literal::Int(1), span)),
                    right: Box::new(Expr::Literal(Literal::Int(2), span)),
                    span,
                },
                "BinaryOp",
            ),
            (
                Expr::Index {
                    array: Box::new(Expr::Var("arr".to_string().into(), span)),
                    indices: vec![],
                    span,
                },
                "Index",
            ),
            (
                Expr::Comprehension {
                    body: Box::new(Expr::Var("x".to_string().into(), span)),
                    var: "x".to_string().into(),
                    iter: Box::new(Expr::Var("iter".to_string().into(), span)),
                    filter: None,
                    span,
                },
                "Comprehension",
            ),
            (
                Expr::Generator {
                    body: Box::new(Expr::Var("x".to_string().into(), span)),
                    var: "x".to_string().into(),
                    iter: Box::new(Expr::Var("iter".to_string().into(), span)),
                    filter: None,
                    span,
                },
                "Generator",
            ),
        ];

        for (expr, expected_name) in test_cases {
            let name = expr_variant_name(&expr);
            assert_eq!(
                name, expected_name,
                "expr_variant_name should return '{}' for {:?}",
                expected_name, expr
            );
            // Verify the name doesn't look like a discriminant
            assert!(
                !name.starts_with("Variant("),
                "expr_variant_name should not return discriminant format: {}",
                name
            );
        }
    }

    /// Verify that error messages for unsupported constructs include readable type names.
    /// (Issue #2210, #2229)
    #[test]
    fn test_unsupported_error_messages_are_readable() {
        let span = test_span();

        // Create an unsupported statement (Try)
        let try_stmt = Stmt::Try {
            try_block: Block {
                stmts: vec![],
                span,
            },
            catch_var: Some("e".to_string()),
            catch_block: Some(Block {
                stmts: vec![],
                span,
            }),
            else_block: None,
            finally_block: None,
            span,
        };

        let mut specializer = FunctionSpecializer::new_for_tests(HashMap::new(), &[]);
        let result = specializer.compile_stmt(&try_stmt);

        // Verify the error message contains the readable name
        assert!(
            matches!(&result, Err(SpecializationError::Unsupported(_))),
            "Expected Unsupported error for Try statement, got {:?}",
            result
        );
        if let Err(SpecializationError::Unsupported(msg)) = result {
            assert!(
                msg.contains("Try"),
                "Error message should contain 'Try', got: {}",
                msg
            );
            assert!(
                !msg.contains("Variant("),
                "Error message should not contain discriminant format: {}",
                msg
            );
        }

        // Test unsupported expression (Comprehension)
        let comprehension = Expr::Comprehension {
            body: Box::new(Expr::Var("x".to_string().into(), span)),
            var: "x".to_string().into(),
            iter: Box::new(Expr::Var("iter".to_string().into(), span)),
            filter: None,
            span,
        };

        let result = specializer.compile_expr(&comprehension);
        assert!(
            matches!(&result, Err(SpecializationError::Unsupported(_))),
            "Expected Unsupported error for Comprehension expression, got {:?}",
            result
        );
        if let Err(SpecializationError::Unsupported(msg)) = result {
            assert!(
                msg.contains("Comprehension"),
                "Error message should contain 'Comprehension', got: {}",
                msg
            );
        }
    }

    #[test]
    fn test_compile_index_preserves_arrayof_element_type() {
        let span = test_span();
        let mut locals = HashMap::new();
        locals.insert(
            "arr".to_string(),
            ValueType::ArrayOf(ArrayElementType::F64, None),
        );
        let mut specializer = FunctionSpecializer::new_for_tests(locals, &[]);

        let array_expr = Expr::Var("arr".to_string().into(), span);
        let indices = vec![Expr::Literal(Literal::Int(1), span)];
        let result = specializer
            .compile_index(&array_expr, &indices)
            .expect("compile index");

        assert_eq!(result, ValueType::F64);
    }

    // ---- Issue #6346: FieldAssign / FieldAccess / DestructuringAssign ----

    fn literal_destructuring_stmt() -> Stmt {
        let span = test_span();
        Stmt::DestructuringAssign {
            targets: vec!["x".to_string(), "y".to_string()],
            value: Expr::TupleLiteral {
                elements: vec![
                    Expr::Literal(Literal::Int(1), span),
                    Expr::Literal(Literal::Int(2), span),
                ],
                span,
            },
            span,
        }
    }

    #[test]
    fn test_literal_destructuring_assign_specializes_targets_10444() {
        let mut spec = FunctionSpecializer::new_for_tests(HashMap::new(), &[]);
        spec.compile_stmt(&literal_destructuring_stmt())
            .expect("literal destructuring should specialize");

        assert_eq!(spec.locals.get("x"), Some(&ValueType::I64));
        assert_eq!(spec.locals.get("y"), Some(&ValueType::I64));
    }

    #[test]
    fn test_literal_destructuring_tail_returns_tuple_10444() {
        let span = test_span();
        let mut spec = FunctionSpecializer::new_for_tests(HashMap::new(), &[]);
        spec.compile_function_body(&Block {
            stmts: vec![literal_destructuring_stmt()],
            span,
        })
        .expect("tail destructuring should specialize");

        assert_eq!(spec.current_return_type, ValueType::Tuple);
        assert!(spec
            .code
            .iter()
            .any(|instr| matches!(instr, Instr::NewTuple(2))));
        assert!(spec
            .code
            .iter()
            .any(|instr| matches!(instr, Instr::ReturnAny)));
    }

    #[test]
    fn test_nonliteral_destructuring_tail_specializes_10464() {
        let span = test_span();
        let mut locals = HashMap::new();
        locals.insert("rhs".to_string(), ValueType::Tuple);
        let mut spec = FunctionSpecializer::new_for_tests(locals, &[]);
        spec.compile_function_body(&Block {
            stmts: vec![Stmt::DestructuringAssign {
                targets: vec!["x".to_string(), "y".to_string()],
                value: Expr::Var("rhs".to_string().into(), span),
                span,
            }],
            span,
        })
        .expect("nonliteral destructuring should specialize without fallback");

        assert_eq!(spec.current_return_type, ValueType::Tuple);
        assert_eq!(
            spec.code
                .iter()
                .filter(|instr| matches!(instr, Instr::TupleGet))
                .count(),
            2
        );
        assert!(spec
            .code
            .iter()
            .any(|instr| matches!(instr, Instr::ReturnAny)));
    }

    fn point_struct_defs(is_mutable: bool) -> Vec<StructDefInfo> {
        vec![StructDefInfo {
            name: "Point6346".to_string(),
            is_mutable,
            fields: vec![
                ("x".to_string(), ValueType::F64),
                ("y".to_string(), ValueType::F64),
            ],
            field_julia_types: Vec::new(),
            parent_type: None,
        }]
    }

    /// `obj.field` read on a known struct resolves the field index/type
    /// statically and emits `GetField`. (Issue #6346)
    #[test]
    fn test_field_access_read_resolves_to_getfield() {
        let span = test_span();
        let defs = point_struct_defs(true);
        let mut locals = HashMap::new();
        locals.insert("p".to_string(), ValueType::Struct(0));
        let mut spec = FunctionSpecializer::new_for_tests(locals, &defs);

        let expr = Expr::FieldAccess {
            object: Box::new(Expr::Var("p".to_string().into(), span)),
            field: "y".to_string().into(),
            span,
        };
        let ty = spec
            .compile_expr(&expr)
            .expect("field read should specialize");
        assert_eq!(ty, ValueType::F64, "y is declared ::Float64");
        assert!(
            spec.code.iter().any(|i| matches!(i, Instr::GetField(1))),
            "expected GetField(1) for field y, got {:?}",
            spec.code
        );
    }

    /// `obj.field = value` on a *mutable* struct emits a statically-resolved
    /// `SetField(idx)`. (Issue #6346)
    #[test]
    fn test_field_assign_mutable_struct_emits_setfield() {
        let span = test_span();
        let defs = point_struct_defs(true);
        let mut locals = HashMap::new();
        locals.insert("p".to_string(), ValueType::Struct(0));
        let mut spec = FunctionSpecializer::new_for_tests(locals, &defs);

        let stmt = Stmt::FieldAssign {
            object: "p".to_string(),
            field: "y".to_string(),
            value: Expr::Literal(Literal::Float(3.0), span),
            span,
        };
        spec.compile_stmt(&stmt)
            .expect("mutable field assign should specialize");
        assert!(
            spec.code.iter().any(|i| matches!(i, Instr::SetField(1))),
            "expected SetField(1) for field y, got {:?}",
            spec.code
        );
        assert!(
            spec.code
                .iter()
                .any(|i| matches!(i, Instr::StoreStruct(name) if name == "p")),
            "expected StoreStruct(p) to write the mutated struct back, got {:?}",
            spec.code
        );
    }

    /// Assigning an `Int` literal to a `::Float64` field coerces via `ToF64`,
    /// exactly matching the interpreter's `compile_expr_as`. (Issue #6346)
    #[test]
    fn test_field_assign_coerces_int_to_float_field() {
        let span = test_span();
        let defs = point_struct_defs(true);
        let mut locals = HashMap::new();
        locals.insert("p".to_string(), ValueType::Struct(0));
        let mut spec = FunctionSpecializer::new_for_tests(locals, &defs);

        let stmt = Stmt::FieldAssign {
            object: "p".to_string(),
            field: "x".to_string(),
            value: Expr::Literal(Literal::Int(2), span),
            span,
        };
        spec.compile_stmt(&stmt)
            .expect("coercible field assign should specialize");
        assert!(
            spec.code.iter().any(|i| matches!(i, Instr::ToF64)),
            "expected ToF64 coercion for Int->Float64 field, got {:?}",
            spec.code
        );
        assert!(
            spec.code.iter().any(|i| matches!(i, Instr::SetField(0))),
            "expected SetField(0) for field x, got {:?}",
            spec.code
        );
    }

    /// Field assignment on an *immutable* struct must fall back to the
    /// interpreter (the typed `SetField` fast path is mutable-only). (Issue #6346)
    #[test]
    fn test_field_assign_immutable_struct_falls_back() {
        let span = test_span();
        let defs = point_struct_defs(false);
        let mut locals = HashMap::new();
        locals.insert("p".to_string(), ValueType::Struct(0));
        let mut spec = FunctionSpecializer::new_for_tests(locals, &defs);

        let stmt = Stmt::FieldAssign {
            object: "p".to_string(),
            field: "x".to_string(),
            value: Expr::Literal(Literal::Float(1.0), span),
            span,
        };
        let result = spec.compile_stmt(&stmt);
        assert!(
            matches!(result, Err(SpecializationError::Unsupported(_))),
            "immutable field assign should not specialize, got {:?}",
            result
        );
    }

    /// A `where`-clause binder whose name collides with a builtin type name
    /// (`h(x::Float64) where {Float64} = Float64(2)`) must be registered as a
    /// specializer local so the body call routes through the per-call TypeVar
    /// binding (dynamic `LoadAny` + `CallFunctionVariable`), NOT the baked-in
    /// builtin `Float64` conversion/type object (Issue #10407, extending the
    /// Issue #10146 local-callee gate to where binders).
    #[test]
    fn test_issue_10407_where_binder_shadows_builtin_type() {
        let span = test_span();
        let func = crate::ir::core::Function {
            name: "h".to_string(),
            params: vec![crate::ir::core::TypedParam::new(
                "x".to_string(),
                Some(crate::types::JuliaType::TypeVar(
                    "Float64".to_string(),
                    None,
                )),
                span,
            )],
            kwparams: vec![],
            type_params: vec![crate::types::TypeParam::new("Float64".to_string())],
            return_type: None,
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::Call {
                        function: "Float64".to_string().into(),
                        args: vec![Expr::Literal(Literal::Int(2), span)],
                        kwargs: vec![],
                        splat_mask: vec![],
                        kwargs_splat_mask: vec![],
                        span,
                    }),
                    span,
                }],
                span,
            },
            is_base_extension: false,
            is_runtime_eval: false,
            span,
            new_struct_name: None,
        };
        let mut type_object_names = HashSet::new();
        type_object_names.insert("Float64".to_string());

        let result = specialize_function(
            &func,
            &[ValueType::I64],
            &[],
            &type_object_names,
            None,
            false,
            false,
        )
        .expect("where-binder shadowed body should specialize dynamically");

        assert!(
            result
                .code
                .iter()
                .any(|i| matches!(i, Instr::LoadAny(name) if name == "Float64")),
            "body must load the where binder dynamically (frame type binding), got {:?}",
            result.code
        );
        assert!(
            result
                .code
                .iter()
                .any(|i| matches!(i, Instr::CallFunctionVariable(1))),
            "body must call through the loaded binder value, got {:?}",
            result.code
        );
    }

    #[test]
    fn test_issue_10146_local_callee_shadows_numeric_constructor() {
        let span = test_span();
        let mut locals = HashMap::new();
        locals.insert("Float64".to_string(), ValueType::Any);
        let mut spec = FunctionSpecializer::new_for_tests(locals, &[]);

        let expr = Expr::Call {
            function: "Float64".to_string().into(),
            args: vec![Expr::Literal(Literal::Int(2), span)],
            kwargs: vec![],
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        };
        let ty = spec
            .compile_expr(&expr)
            .expect("local callable numeric-constructor shadow should specialize dynamically");

        assert_eq!(ty, ValueType::Any);
        assert!(
            matches!(
                spec.code.as_slice(),
                [
                    Instr::PushI64(2),
                    Instr::LoadAny(name),
                    Instr::CallFunctionVariable(1),
                ] if name == "Float64"
            ),
            "local callee must shadow the Float64 builtin constructor, got {:?}",
            spec.code
        );
    }

    /// Shadowing matrix over representative name-keyed `compile_call` fast
    /// paths (Issue #10418, prevention for #10146): a local binding whose
    /// name collides with a specializer builtin arm must compile as a
    /// callable value (`LoadAny` + `CallFunctionVariable`), never as the
    /// builtin instruction. The unshadowed leg pins that each matrix name
    /// still reaches its name-keyed arm, so the shadowed leg keeps guarding
    /// real fast paths. When adding a new name-keyed arm to
    /// `vm/specialize/expr.rs::compile_call`, extend this matrix (see
    /// docs/vm/CHECKLISTS.md "Runtime Specializer Name-Keyed Callee Fast
    /// Paths").
    #[test]
    fn test_issue_10418_local_callee_shadowing_matrix_over_specializer_fast_paths() {
        use crate::builtins::BuiltinId;

        let span = test_span();
        // (callee name, argument literal, probe for the builtin-arm instruction)
        let cases: [(&str, Literal, fn(&Instr) -> bool); 4] = [
            ("Float64", Literal::Int(2), |i| matches!(i, Instr::ToF64)),
            // Issues #11198/#11215: a Float64 argument no longer takes the
            // unconditional primitive fast path (it could truncate a
            // fractional value without raising `InexactError`), so this
            // probe uses a Bool argument — the exact/method-free case the
            // fast path still legitimately covers (see
            // `expr.rs::emit_exact_to_i64`).
            ("Int64", Literal::Bool(true), |i| {
                matches!(i, Instr::BoolToI64)
            }),
            ("sqrt", Literal::Float(4.0), |i| matches!(i, Instr::SqrtF64)),
            ("round", Literal::Float(2.5), |i| {
                matches!(i, Instr::CallBuiltin(BuiltinId::Round, 1))
            }),
        ];

        for (callee, arg, is_builtin_instr) in cases {
            let call = Expr::Call {
                function: callee.to_string().into(),
                args: vec![Expr::Literal(arg, span)],
                kwargs: vec![],
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span,
            };

            // Unshadowed: the name-keyed arm emits its builtin instruction.
            let mut unshadowed = FunctionSpecializer::new_for_tests(HashMap::new(), &[]);
            unshadowed
                .compile_expr(&call)
                .unwrap_or_else(|e| panic!("unshadowed {callee} call should specialize: {e:?}"));
            assert!(
                unshadowed.code.iter().any(is_builtin_instr),
                "unshadowed {callee} must reach its name-keyed builtin arm \
                 (arm removed or renamed? update this matrix), got {:?}",
                unshadowed.code
            );

            // Shadowed: a local binding of the same name wins, matching the
            // stack compiler's callable-variable resolution (Julia scope).
            let mut locals = HashMap::new();
            locals.insert(callee.to_string(), ValueType::Any);
            let mut shadowed = FunctionSpecializer::new_for_tests(locals, &[]);
            let ty = shadowed.compile_expr(&call).unwrap_or_else(|e| {
                panic!("shadowed {callee} call should compile as a callable value: {e:?}")
            });
            assert_eq!(ty, ValueType::Any);
            assert!(
                matches!(
                    shadowed.code.as_slice(),
                    [_, Instr::LoadAny(name), Instr::CallFunctionVariable(1)] if name == callee
                ),
                "local {callee} binding must shadow the builtin fast path \
                 with LoadAny + CallFunctionVariable, got {:?}",
                shadowed.code
            );
            assert!(
                !shadowed.code.iter().any(is_builtin_instr),
                "shadowed {callee} must not emit the builtin instruction, got {:?}",
                shadowed.code
            );
        }
    }

    /// An n-ary operator application `*(a, b, c)` (how the parser spells the
    /// chained product `a * b * c`) folds left through the typed binary-op path
    /// instead of aborting specialization. (Issue #6346)
    #[test]
    fn test_nary_mul_operator_call_folds_to_typed_ops() {
        let span = test_span();
        let mut locals = HashMap::new();
        locals.insert("a".to_string(), ValueType::F64);
        locals.insert("b".to_string(), ValueType::F64);
        let mut spec = FunctionSpecializer::new_for_tests(locals, &[]);

        // `*(a, b, 2.0)` — three operands, F64 throughout.
        let expr = Expr::Call {
            function: "*".to_string().into(),
            args: vec![
                Expr::Var("a".to_string().into(), span),
                Expr::Var("b".to_string().into(), span),
                Expr::Literal(Literal::Float(2.0), span),
            ],
            kwargs: vec![],
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        };
        let ty = spec
            .compile_expr(&expr)
            .expect("n-ary * call should specialize");
        assert_eq!(ty, ValueType::F64);
        let mul_count = spec
            .code
            .iter()
            .filter(|i| matches!(i, Instr::MulF64))
            .count();
        assert_eq!(
            mul_count, 2,
            "three-operand product folds to two MulF64, got {:?}",
            spec.code
        );
    }

    /// An n-ary operator call on non-numeric operands stays on the interpreter
    /// fallback (the typed fold only covers primitive numerics). (Issue #6346)
    #[test]
    fn test_nary_operator_call_non_numeric_falls_back() {
        let span = test_span();
        let mut locals = HashMap::new();
        locals.insert("s".to_string(), ValueType::Str);
        locals.insert("t".to_string(), ValueType::Str);
        let mut spec = FunctionSpecializer::new_for_tests(locals, &[]);

        // String concatenation `*(s, t)` must not become a typed numeric MulF64.
        let expr = Expr::Call {
            function: "*".to_string().into(),
            args: vec![
                Expr::Var("s".to_string().into(), span),
                Expr::Var("t".to_string().into(), span),
            ],
            kwargs: vec![],
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        };
        let result = spec.compile_expr(&expr);
        assert!(
            matches!(result, Err(SpecializationError::Unsupported(_))),
            "string `*` should fall back, got {:?}",
            result
        );
    }

    // ---- Issue #6561: tuple-literal temporary element-type tracking ----

    /// Build a specializer with `a` and `b` bound to `I64`, then compile the
    /// desugared swap statements `t = (b, a)` and return the specializer so a
    /// caller can drive `compile_index` against the tracked temporary.
    fn swap_temp_specializer<'a>() -> FunctionSpecializer<'a> {
        let mut locals = HashMap::new();
        locals.insert("a".to_string(), ValueType::I64);
        locals.insert("b".to_string(), ValueType::I64);
        FunctionSpecializer::new_for_tests(locals, &[])
    }

    fn tuple_assign(var: &str, elems: Vec<Expr>) -> Stmt {
        Stmt::Assign {
            var: var.to_string(),
            value: Expr::TupleLiteral {
                elements: elems,
                span: test_span(),
            },
            span: test_span(),
        }
    }

    /// A constant index into a tracked tuple temporary returns the recorded
    /// element type (so the caller emits a typed `Store*`) instead of widening
    /// to `Any`. The recorded type matches the `IndexLoad` result tag exactly,
    /// so no dynamic coercion is emitted. (Issue #6561)
    #[test]
    fn test_tuple_temp_index_read_sharpens_to_typed() {
        let span = test_span();
        let mut spec = swap_temp_specializer();
        spec.compile_stmt(&tuple_assign(
            "t",
            vec![
                Expr::Var("b".to_string().into(), span),
                Expr::Var("a".to_string().into(), span),
            ],
        ))
        .expect("tuple temp assignment should specialize");

        let before = spec.code.len();
        let ty = spec.compile_index(
            &Expr::Var("t".to_string().into(), span),
            &[Expr::Literal(Literal::Int(1), span)],
        );
        assert_eq!(
            ty.expect("index should specialize"),
            ValueType::I64,
            "tracked tuple read should return the recorded element type"
        );
        assert!(
            spec.code[before..]
                .iter()
                .any(|i| matches!(i, Instr::IndexLoad(1))),
            "tracked tuple read should still load the element via IndexLoad: {:?}",
            &spec.code[before..]
        );
        // The element tag already matches the recorded type, so the sharpen
        // does NOT pay for a redundant dynamic coercion.
        assert!(
            !spec.code[before..]
                .iter()
                .any(|i| matches!(i, Instr::DynamicToI64 | Instr::DynamicToF64)),
            "typed tuple read must not emit a redundant coercion: {:?}",
            &spec.code[before..]
        );
    }

    /// Indexing a `Tuple`-typed local that was NOT tracked (no recorded element
    /// types) stays on the generic `Any` path. (Issue #6561)
    #[test]
    fn test_untracked_tuple_index_stays_any() {
        let span = test_span();
        let mut locals = HashMap::new();
        locals.insert("t".to_string(), ValueType::Tuple);
        let mut spec = FunctionSpecializer::new_for_tests(locals, &[]);

        let ty = spec
            .compile_index(
                &Expr::Var("t".to_string().into(), span),
                &[Expr::Literal(Literal::Int(1), span)],
            )
            .expect("index should specialize");
        assert_eq!(ty, ValueType::Any, "untracked tuple index must stay Any");
    }

    /// Reassigning the temporary to a non-tuple value invalidates the tracked
    /// element types so a later index no longer sharpens. (Issue #6561)
    #[test]
    fn test_tuple_temp_invalidated_on_non_tuple_reassign() {
        let span = test_span();
        let mut spec = swap_temp_specializer();
        spec.compile_stmt(&tuple_assign(
            "t",
            vec![
                Expr::Var("b".to_string().into(), span),
                Expr::Var("a".to_string().into(), span),
            ],
        ))
        .expect("tuple temp assignment should specialize");
        // Overwrite `t` with a plain integer; tracking must be dropped.
        spec.compile_stmt(&Stmt::Assign {
            var: "t".to_string(),
            value: Expr::Literal(Literal::Int(7), span),
            span,
        })
        .expect("reassignment should specialize");

        let ty = spec
            .compile_index(
                &Expr::Var("t".to_string().into(), span),
                &[Expr::Literal(Literal::Int(1), span)],
            )
            .expect("index should specialize");
        assert_eq!(ty, ValueType::Any, "invalidated tuple index must stay Any");
    }

    /// A tracked tuple element whose type is outside the sound-coercion numeric
    /// subset (e.g. `Str`) is left on the generic `Any` path. (Issue #6561)
    #[test]
    fn test_tuple_temp_non_numeric_element_stays_any() {
        let span = test_span();
        let mut locals = HashMap::new();
        locals.insert("s".to_string(), ValueType::Str);
        locals.insert("n".to_string(), ValueType::I64);
        let mut spec = FunctionSpecializer::new_for_tests(locals, &[]);
        spec.compile_stmt(&tuple_assign(
            "t",
            vec![
                Expr::Var("s".to_string().into(), span),
                Expr::Var("n".to_string().into(), span),
            ],
        ))
        .expect("tuple temp assignment should specialize");

        // Element 1 is Str -> generic Any path (no sharpen); element 2 is I64
        // -> sharpened to a typed read.
        let str_ty = spec
            .compile_index(
                &Expr::Var("t".to_string().into(), span),
                &[Expr::Literal(Literal::Int(1), span)],
            )
            .expect("index should specialize");
        assert_eq!(str_ty, ValueType::Any, "Str element must stay Any");

        let num_ty = spec
            .compile_index(
                &Expr::Var("t".to_string().into(), span),
                &[Expr::Literal(Literal::Int(2), span)],
            )
            .expect("index should specialize");
        assert_eq!(num_ty, ValueType::I64, "I64 element must be sharpened");
    }

    /// The runtime specializer infers ComplexF64 arithmetic and `abs2` calls
    /// when operands are known `ComplexF64` locals, so the typed-loop recognizer
    /// can see through parser-emitted n-ary operator calls. (Issue #10567)
    #[test]
    fn infer_call_complex_f64() {
        let span = test_span();
        let mut locals = HashMap::new();
        locals.insert("z".to_string(), ValueType::ComplexF64);
        let spec = FunctionSpecializer::new_for_tests(locals, &[]);

        let call = |function: &str, args: Vec<Expr>| {
            let len = args.len();
            Expr::Call {
                function: function.to_string().into(),
                args,
                kwargs: vec![],
                splat_mask: vec![false; len],
                kwargs_splat_mask: vec![],
                span,
            }
        };
        let z = || Expr::Var("z".into(), span);

        assert_eq!(
            spec.infer_literal_type(&call("*", vec![z(), z()])),
            Some(ValueType::ComplexF64),
            "*(z, z)"
        );
        assert_eq!(
            spec.infer_literal_type(&call("+", vec![z(), z()])),
            Some(ValueType::ComplexF64),
            "+(z, z)"
        );
        assert_eq!(
            spec.infer_literal_type(&call("-", vec![z(), z()])),
            Some(ValueType::ComplexF64),
            "-(z, z)"
        );
        assert_eq!(
            spec.infer_literal_type(&call("+", vec![z(), z(), z()])),
            Some(ValueType::ComplexF64),
            "n-ary +(z, z, z)"
        );
        assert_eq!(
            spec.infer_literal_type(&call("abs2", vec![z()])),
            Some(ValueType::F64),
            "abs2(z)"
        );

        // A non-complex operand should not infer a specialized type.
        let mut locals_i64 = HashMap::new();
        locals_i64.insert("x".to_string(), ValueType::I64);
        let spec_i64 = FunctionSpecializer::new_for_tests(locals_i64, &[]);
        assert_eq!(
            spec_i64.infer_literal_type(&call(
                "*",
                vec![Expr::Var("x".into(), span), Expr::Var("x".into(), span)]
            )),
            None,
            "*(x, x) with I64 locals stays None"
        );
    }

    /// Split-slot tracking for `ComplexF64` locals generates stable re/im slot
    /// names and records them in `complex_splits`. (Issue #10567)
    #[test]
    fn test_complex_split_slot_tracking_10567() {
        let mut locals = HashMap::new();
        locals.insert("z".to_string(), ValueType::ComplexF64);
        let mut spec = FunctionSpecializer::new_for_tests(locals, &[]);

        assert!(
            !spec.is_complex_split("z"),
            "fresh specializer has no split"
        );
        assert!(!spec.is_complex_split("other"));

        let (re, im) = spec.ensure_complex_split("z");
        assert_eq!(re, "__sjulia_cx_re_z");
        assert_eq!(im, "__sjulia_cx_im_z");
        assert!(spec.is_complex_split("z"));

        // Repeated calls return the same names (stable across the function).
        let (re2, im2) = spec.ensure_complex_split("z");
        assert_eq!(re, re2);
        assert_eq!(im, im2);

        // Different names get different slots.
        let (re_w, im_w) = spec.ensure_complex_split("w");
        assert_eq!(re_w, "__sjulia_cx_re_w");
        assert_eq!(im_w, "__sjulia_cx_im_w");
    }

    /// Split-slot SROA for ComplexF64 produces LoadF64/StoreF64 against the
    /// generated re/im slots instead of boxing intermediate Complex values.
    /// (Issue #10567)
    #[test]
    fn test_complex_f64_split_slot_codegen_10567() {
        let span = test_span();

        let c_var = || Expr::Var("c".into(), span);
        let z_var = || Expr::Var("z".into(), span);
        let body = Block {
            stmts: vec![
                Stmt::Assign {
                    var: "z".to_string(),
                    value: Expr::BinaryOp {
                        op: BinaryOp::Mul,
                        left: Box::new(c_var()),
                        right: Box::new(c_var()),
                        span,
                    },
                    span,
                },
                Stmt::Expr {
                    expr: Expr::Call {
                        function: "abs2".to_string().into(),
                        args: vec![z_var()],
                        kwargs: vec![],
                        splat_mask: vec![false],
                        kwargs_splat_mask: vec![],
                        span,
                    },
                    span,
                },
            ],
            span,
        };

        let func = Function {
            name: "f".to_string(),
            params: vec![TypedParam::new("c".to_string(), None, span)],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body,
            is_base_extension: false,
            is_runtime_eval: false,
            span,
            new_struct_name: None,
        };

        let result = specialize_function(
            &func,
            &[ValueType::ComplexF64],
            &[],
            &std::collections::HashSet::new(),
            None,
            false,
            false,
        )
        .expect("complex specialization should succeed");

        assert_eq!(result.return_type, ValueType::F64);
        assert!(
            result
                .code
                .iter()
                .any(|i| matches!(i, Instr::LoadF64(name) if name == "__sjulia_cx_re_c")),
            "expected split LoadF64 of c.real, got {:?}",
            result.code
        );
        assert!(
            result
                .code
                .iter()
                .any(|i| matches!(i, Instr::LoadF64(name) if name == "__sjulia_cx_im_c")),
            "expected split LoadF64 of c.imag, got {:?}",
            result.code
        );
        assert!(
            result
                .code
                .iter()
                .any(|i| matches!(i, Instr::StoreF64(name) if name == "__sjulia_cx_re_z")),
            "expected split StoreF64 of z.real, got {:?}",
            result.code
        );
        assert!(
            result
                .code
                .iter()
                .any(|i| matches!(i, Instr::StoreF64(name) if name == "__sjulia_cx_im_z")),
            "expected split StoreF64 of z.imag, got {:?}",
            result.code
        );
    }

    /// The split-slot `abs2(z::ComplexF64)` sequence must compute re^2 + im^2
    /// by loading each field EXACTLY ONCE (Issue #10799: `LoadF64(re); Dup;
    /// Mul; LoadF64(im); Dup; Mul; Add` — no temp spill, so the shared
    /// Instr-level peephole fuses each `Load;Dup;Mul` triple into
    /// `LoadSquareF64Slot`, letting the typed-loop predecoder's
    /// `fuse_typed_loop_ops` further fuse the pair + `AddF64` into ONE
    /// `PushSumSquaresF64Slots`, matching the static compiler's SROA'd
    /// shape). A pre-#10799 version of this rewrite spilled the computed
    /// im^2 to a temp before reusing it (an even older version before that,
    /// fixed by #10567, accidentally duplicated `im` and computed
    /// re + 2*im^2 instead of re^2 + im^2 — this test's real invariant,
    /// preserved across both rewrites, is that re and im are each read
    /// exactly once).
    #[test]
    fn test_complex_abs2_split_slot_sequence_10567() {
        let span = test_span();
        let z_var = || Expr::Var("z".into(), span);

        let body = Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Call {
                    function: "abs2".to_string().into(),
                    args: vec![z_var()],
                    kwargs: vec![],
                    splat_mask: vec![false],
                    kwargs_splat_mask: vec![],
                    span,
                }),
                span,
            }],
            span,
        };

        let func = Function {
            name: "f_abs2".to_string(),
            params: vec![TypedParam::new("z".to_string(), None, span)],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body,
            is_base_extension: false,
            is_runtime_eval: false,
            span,
            new_struct_name: None,
        };

        let result = specialize_function(
            &func,
            &[ValueType::ComplexF64],
            &[],
            &std::collections::HashSet::new(),
            None,
            false,
            false,
        )
        .expect("abs2 specialization should succeed");

        assert_eq!(result.return_type, ValueType::F64);

        let re_z = "__sjulia_cx_re_z";
        let im_z = "__sjulia_cx_im_z";
        let split_pos = result
            .code
            .windows(3)
            .position(
                |w| matches!(w, [Instr::LoadF64(r), Instr::DupF64, Instr::MulF64] if r == re_z),
            )
            .expect("expected LoadF64(re_z); Dup; Mul triple");

        let window = &result.code[split_pos..split_pos + 7];
        assert!(
            matches!(&window[0], Instr::LoadF64(r) if r == re_z),
            "expected LoadF64(re_z), got {:?}",
            window[0]
        );
        assert!(
            matches!(window[1], Instr::DupF64),
            "expected DupF64, got {:?}",
            window[1]
        );
        assert!(
            matches!(window[2], Instr::MulF64),
            "expected MulF64, got {:?}",
            window[2]
        );
        assert!(
            matches!(&window[3], Instr::LoadF64(i) if i == im_z),
            "expected LoadF64(im_z), got {:?}",
            window[3]
        );
        assert!(
            matches!(window[4], Instr::DupF64),
            "expected DupF64, got {:?}",
            window[4]
        );
        assert!(
            matches!(window[5], Instr::MulF64),
            "expected MulF64, got {:?}",
            window[5]
        );
        assert!(
            matches!(window[6], Instr::AddF64),
            "expected AddF64, got {:?}",
            window[6]
        );
    }

    /// Returning a `ComplexF64` parameter materializes the split `[re, im]` pair
    /// back into a boxed `Complex` struct before the return instruction.
    /// (Issue #10567)
    #[test]
    fn test_complex_f64_return_materializes_boxed_struct_10567() {
        let span = test_span();

        let body = Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Var("c".into(), span)),
                span,
            }],
            span,
        };

        let func = Function {
            name: "f".to_string(),
            params: vec![TypedParam::new("c".to_string(), None, span)],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body,
            is_base_extension: false,
            is_runtime_eval: false,
            span,
            new_struct_name: None,
        };

        let result = specialize_function(
            &func,
            &[ValueType::ComplexF64],
            &[],
            &std::collections::HashSet::new(),
            None,
            false,
            false,
        )
        .expect("ComplexF64 return specialization should succeed");

        assert_eq!(result.return_type, ValueType::ComplexF64);

        let tail = &result.code[(result.code.len().saturating_sub(4))..];
        assert!(
            matches!(
                tail,
                [
                    Instr::LoadF64(_),
                    Instr::LoadF64(_),
                    Instr::NewParametricStruct(name, 2),
                    Instr::ReturnAny,
                ] if name == "Complex"
            ),
            "expected split-load + NewParametricStruct(\"Complex\", 2) + ReturnAny, got {:?}",
            result.code
        );
    }
}
