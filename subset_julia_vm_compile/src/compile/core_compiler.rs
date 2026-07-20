//! CoreCompiler struct definition and basic methods.
//!
//! This module defines the `CoreCompiler` struct which holds all state needed
//! during compilation of a function or module body, including:
//! - Emitted instructions
//! - Local variable type tracking
//! - Method tables for dispatch
//! - Loop/finally context stacks
//! - Closure capture tracking
//!
//! Supporting types `LoopContext` and `FinallyContext` are also defined here.

use std::collections::{HashMap, HashSet};

use crate::bytecode::{DynamicCallCandidate, Instr, ResolvedFunctionOperands, ValueType};
use crate::ir::core::{Block, Function};
use crate::span::Span;
use crate::types::{JuliaType, TypeParam};

use super::context::SharedCompileContext;
use super::method_table::MethodTable;
use super::module_alias::{
    bound_module_aliases, build_canonical_module_alias_states, imported_binding_names,
    ImportedBindingResolution,
};
pub(super) use super::module_alias::{
    build_live_import_binding_states, ImportBindingKind, ModuleAliasState, ResolvedUsingImport,
};
use super::type_helpers::julia_type_to_value_type;
use super::types::{self, CResult};

/// Loop context tracking patch points for break/continue
#[derive(Debug)]
pub(super) struct LoopContext {
    /// Instruction indices for loop exits (break)
    pub exit_patches: Vec<usize>,
    /// Instruction indices for loop continues
    pub continue_patches: Vec<usize>,
}

/// Finally block context for tracking pending finally blocks.
/// Used to ensure finally blocks execute even with return/break/continue.
#[derive(Debug, Clone)]
pub(super) struct FinallyContext {
    /// The finally block IR to execute
    pub finally_block: Block,
    /// Loop depth when this finally was pushed (for break/continue scoping)
    pub loop_depth: usize,
    pub fresh_locals: Vec<String>,
    pub explicit_locals: Vec<String>,
    pub declared_globals: Vec<String>,
    pub enclosing_scope_locals: HashSet<String>,
}

#[derive(Debug, Clone)]
pub(super) struct ScopeCleanupContext {
    pub names: Vec<String>,
    pub shadows: Vec<ShadowedLocal>,
    /// Number of explicit lexical scopes that a non-local exit must close.
    ///
    /// This is separate from `names`/`shadows`: module/main hard scopes use
    /// VM-owned lexical bindings instead of frame-zero save/restore slots
    /// (Issues #11569/#9784).
    pub lexical_scope_count: usize,
    pub loop_depth: usize,
    /// False for the loop construct's own lifetime scope: break converges on
    /// its normal exit bytecode and continue keeps it active. Nested lets/try
    /// clauses set this true and must clean up before either jump.
    pub cleanup_on_loop_exit: bool,
    pub nonlocal_pop_handler: bool,
    pub nonlocal_pop_caught_exception: bool,
}

/// Compile-time alias facts hidden by one VM-owned lexical declaration owner.
///
/// Alias maps describe the currently visible binding, so they must follow the
/// same push/pop lifetime as the runtime lexical slot. Keeping the saved facts
/// on the owner stack also handles nested same-named owners without relying on
/// every loop/comprehension/clause caller to remember a separate snapshot.
struct ExplicitLexicalScopeState {
    names: HashSet<String>,
    hidden_aliases: Vec<ExplicitLexicalAliasState>,
}

struct ExplicitLexicalAliasState {
    name: String,
    function_alias: Option<String>,
    lexical_function_table: Option<String>,
    type_value_alias: Option<JuliaType>,
    module_alias: Option<String>,
    inherited_declared_global: bool,
}

pub(super) struct CoreCompiler<'a> {
    pub(super) code: Vec<Instr>,
    pub(super) source_map: Vec<Option<Span>>,
    current_span: Option<Span>,
    pub(super) locals: HashMap<String, ValueType>,
    /// Locals that have been initialized by code emitted before the current
    /// point. `locals` is pre-populated by whole-block inference, so it can also
    /// contain future sibling-scope names that do not have a runtime slot yet.
    pub(super) initialized_locals: HashSet<String>,
    /// JuliaType tracking for parametric types that ValueType cannot represent.
    ///
    /// `ValueType::Tuple` is non-parametric — it only represents "some tuple" without
    /// element type information. When a tuple literal like `(42, 10)` is assigned to a
    /// variable, the `locals` map records it as `ValueType::Tuple`, losing the precise
    /// `Tuple{Int64, Int64}` type needed for parametric dispatch.
    ///
    /// This map stores the full `JuliaType` (e.g., `JuliaType::TupleOf([Int64, Int64])`)
    /// so that `infer_julia_type()` can recover precise parametric information when
    /// building argument type lists for method dispatch.
    ///
    /// **Lookup priority in `infer_julia_type()`:**
    /// 1. Check `julia_type_locals` first (parametric types like `Tuple{Int64, Int64}`)
    /// 2. Fall back to `locals` / `global_types` (ValueType-based inference)
    ///
    /// **Currently tracked:** Tuple literals assigned to variables (Issue #1748).
    /// Could be extended to other parametric types (NamedTuple, etc.) if needed.
    pub(super) julia_type_locals: HashMap<String, JuliaType>,
    /// Variables whose current `ValueType::ArrayOf(ArrayElementType::Any,
    /// Some(n))` is a *genuinely* `Any`-elemented array (upstream `Vector{Any}`
    /// / `Matrix{Any}` / `Array{Any,N}`, a concrete dispatchable type) rather
    /// than a comprehension's "rank known, element type unresolved" placeholder
    /// (Issue #6817).
    ///
    /// Both producers share the exact same `ArrayOf(Any, Some(n))` shape, and
    /// `infer_julia_type`'s `Expr::Var` bridge cannot tell them apart from the
    /// `ValueType` alone (Issue #10267): a comprehension whose body type could
    /// not be resolved statically must report the bare `Vector`/`Matrix` alias
    /// so element-specific methods defer to runtime dispatch, but a value that
    /// really is `Vector{Any}` (e.g. `Expr.args`) must report the concrete
    /// `VectorOf(Any)`/`MatrixOf(Any)` so a `::Array{Any,N}`-typed method can
    /// still statically bind (Issue #10206).
    ///
    /// Conservative by construction: only the narrow set of producers proven
    /// to be genuinely `Any` (currently: assigning `expr.args`) inserts here;
    /// every other `ArrayOf(Any, Some(n))` producer (comprehensions) is left
    /// unmarked and keeps the safe "unknown, defer to runtime" bridge
    /// behavior. A future array producer that is NOT added here defaults to
    /// the conservative unknown treatment, not a silent wrong static bind.
    ///
    /// Scoped like `julia_type_locals` (saved/restored/cleared at the same
    /// call sites) since a shadowing reassignment or a sibling scope's
    /// same-named variable must not inherit a stale "known Any" marker.
    pub(super) known_any_rank_array_locals: HashSet<String>,
    /// Local/static function aliases such as `g = f`.
    ///
    /// This preserves the original generic function name for compile-time
    /// operations that need method-table lookup rather than just a Function
    /// value, e.g. `invoke(g, Tuple{T}, x)` (Issue #4290).
    pub(super) function_aliases: HashMap<String, String>,
    /// Method tables introduced by a function definition owned by an active
    /// explicit lexical scope. Unlike `function_aliases`, this excludes
    /// ordinary function-valued assignments such as `f = ntuple`.
    pub(super) lexical_function_tables: HashMap<String, String>,
    /// Local/static DataType value aliases such as `sig = Tuple{Number}`.
    ///
    /// This preserves type-object values for compile-time surfaces that still
    /// require a statically known type object, while allowing users to spell the
    /// type through a variable as upstream Julia permits (Issue #4290).
    pub(super) type_value_aliases: HashMap<String, JuliaType>,
    pub(super) method_tables: &'a HashMap<String, MethodTable>,
    /// Module name -> Set of function names defined in that module
    pub(super) module_functions: &'a HashMap<String, HashSet<String>>,
    /// Module name -> Set of exported function names (empty = all exported)
    /// Kept to preserve compile-context contract while export handling evolves.
    #[allow(dead_code)]
    pub(super) module_exports: &'a HashMap<String, HashSet<String>>,
    /// Set of function names that are available via `using` (respects export + selective import)
    pub(super) imported_functions: &'a HashSet<String>,
    /// User-defined globals hidden while compiling Base/prelude code.
    pub(super) hidden_user_globals: HashSet<String>,
    /// Set of module names imported via `using` (for backward compatibility)
    pub(super) usings: &'a HashSet<String>,
    /// Resolved `using` statements for this lexical scope. `None` imports the
    /// module's exports; `Some` is a selective `using M: name` set.
    pub(super) resolved_usings: Vec<ResolvedUsingImport>,
    /// Root modules defined by the user program in Main. Loaded stdlibs and
    /// package dependencies are deliberately absent unless explicitly used.
    pub(super) toplevel_module_bindings: HashSet<String>,
    /// Frame-zero bindings that already exist before this compiler's user-main
    /// fragment runs. A seeded REPL delta must treat a soft-scope assignment to
    /// one of these names as an update of the existing global, even though the
    /// fresh compiler has not emitted an assignment to it in this fragment.
    pub(super) preexisting_global_bindings: HashSet<String>,
    /// Synthetic import-rename target -> lowered path/canonical source/import
    /// span. Keep every occurrence so repeated imports of the same alias do not
    /// make an earlier synthetic assignment look like an ordinary assignment.
    pub(super) import_alias_assignments: HashMap<String, Vec<(String, String, Span)>>,
    /// Source roots suppressed by whole-module renames (`import P as D`).
    pub(super) renamed_only_module_roots: HashSet<String>,
    pub(super) shared_ctx: &'a mut SharedCompileContext,
    pub(super) temp_counter: usize,
    /// Stack of active loops for break/continue support
    pub(super) loop_stack: Vec<LoopContext>,
    /// Stack of active finally blocks for return/break/continue handling
    pub(super) finally_stack: Vec<FinallyContext>,
    pub(super) scope_cleanup_stack: Vec<ScopeCleanupContext>,
    pub(super) lexical_scope_locals: HashSet<String>,
    /// Whether hard scopes in this compiler use the VM's explicit lexical
    /// environment. Enabled for user main/module bodies; function compilers
    /// keep their ordinary frame-local representation.
    pub(super) explicit_lexical_scopes: bool,
    /// Declaration-owner stack used only while emitting bytecode. A name is
    /// routed to `Load/StoreLexical` when the innermost active set owns it.
    explicit_lexical_scope_stack: Vec<ExplicitLexicalScopeState>,
    /// Whether we're in a function body (strict undefined var check) or module/main (lenient)
    pub(super) strict_undefined_check: bool,
    /// Whether a top-level call may target a method body appended dormant for
    /// the current REPL input. Ordinary whole-program compilation does not use
    /// the live-append method-world fence.
    pub(super) repl_source_ordered_top_level_dispatch: bool,
    /// Main-owned concrete type names whose current-input declarations emit
    /// `DefineEvalStruct`. Only these bindings are hidden before their marker;
    /// prior parametric families and imported/module types are already visible.
    pub(super) repl_source_ordered_type_names: HashSet<String>,
    /// Depth of compiler-managed local scopes inside module/main compilation.
    ///
    /// `@testset` bodies are compiled by the module/main compiler, but they are
    /// Julia local scopes. Assignments inside them must be able to shadow
    /// top-level constants such as `pi` instead of being treated as module
    /// constant reassignments (Issue #5991).
    pub(super) local_scope_depth: usize,
    /// Parameters with Any type (no type annotation) - these preserve Any on reassignment
    pub(super) any_params: HashSet<String>,
    /// Parameters with abstract numeric type annotations (Number, Real, Integer, etc.)
    /// Binary operations on these must use runtime dispatch (Issue #2498)
    pub(super) abstract_numeric_params: HashSet<String>,
    /// Static module-value aliases introduced by assignment (`const M = A`).
    pub(super) module_aliases: HashMap<String, String>,
    /// Authoritative source-ordered state for module-valued names introduced
    /// by selective imports, `as` aliases, or nonselective exports.
    pub(super) module_alias_states: HashMap<String, ModuleAliasState>,
    /// Bare names that may be introduced by this scope's `using`/`import`s.
    /// Their selected source binding is runtime state because each statement
    /// activates in source order (Issues #11203/#11216).
    pub(super) imported_bindings: HashSet<String>,
    /// Source-ordered compile-time state used only to emit each runtime binding
    /// transition. References read the hidden runtime state instead.
    pub(super) active_imported_bindings: HashMap<String, ImportedBindingResolution>,
    pub(super) using_import_cursor: usize,
    /// Set of abstract type names (for isa() type checking)
    pub(super) abstract_type_names: &'a HashSet<String>,
    /// Current struct type_id for inner constructor compilation (for new() calls)
    pub(super) current_struct_type_id: Option<usize>,
    /// Current parametric struct base name (e.g., "Rational") for new{T}() calls
    pub(super) current_parametric_struct_name: Option<String>,
    /// Type parameters from current function's where clause (for type binding)
    pub(super) current_type_params: Vec<TypeParam>,
    /// Type param name -> index lookup (Issue #2865)
    pub(super) current_type_param_index: HashMap<String, usize>,
    /// Names of `where`-clause type parameters that are recoverable from a
    /// constructor argument (i.e. they appear in some parameter's type
    /// annotation, like `Bar(x::T)`), so the runtime can bind them by argument
    /// inference. Explicit-only parameters (`Foo{T}(x)` with an untyped `x`)
    /// are excluded because their value is only known from the call site's
    /// `{...}`, which is not yet plumbed into the constructor frame
    /// (Issue #5059).
    pub(super) ctor_arg_bound_type_vars: HashSet<String>,
    /// Inner-constructor `where` variables supplied by its explicit self
    /// application (`Foo{T}`), and therefore available from either
    /// CallStaticParametric or a runtime DataType call (Issue #10959).
    pub(super) ctor_self_bound_type_vars: HashSet<String>,
    /// Variables with mixed F64+I64 types - these need dynamic typing (StoreAny/LoadAny)
    pub(super) mixed_type_vars: HashSet<String>,
    /// Type parameters that come from Val{N} patterns - these should be I64, not DataType
    pub(super) val_type_params: HashSet<String>,
    /// Type parameters that come from Val{true}/Val{false} patterns - these should be Bool
    pub(super) val_bool_params: HashSet<String>,
    /// Type parameters that come from Val{:symbol} patterns - these should be Symbol
    pub(super) val_symbol_params: HashSet<String>,
    /// Current module path (e.g., "Dates") for resolving unqualified struct names
    pub(super) current_module_path: Option<String>,
    /// `baremodule` has implicit Core/Main support but no implicit Base using.
    pub(super) current_module_is_bare: bool,
    /// Names imported into the current module via `using`/`import` (Issue #7575).
    /// An imported name shares one generic function with its source module, so an
    /// unqualified call must keep pooling dispatch candidates across modules and
    /// is excluded from the module-owned-function redirect.
    pub(super) current_module_imports: HashSet<String>,
    /// Module name -> Set of constant names defined in that module's body
    pub(super) module_constants: &'a HashMap<String, HashSet<String>>,
    /// Label positions: label_name -> instruction index (for @label)
    pub(super) label_positions: HashMap<String, usize>,
    /// Goto patches: (instruction_index, target_label_name) (for @goto)
    pub(super) goto_patches: Vec<(usize, String)>,
    /// Definition span starts whose `where`-bound resolution probes were
    /// already emitted by this compiler (Issue #10396). A function defined
    /// inside a top-level block statement (`try`, `@testset`, …) is compiled
    /// by BOTH the `Stmt::FunctionDef` arm (inside any enclosing handler
    /// region — the semantically correct spot) and the later top-level
    /// source-order activation flush; the flush must not re-probe, or a
    /// caught UndefVarError would be re-raised outside the `try`.
    pub(super) where_probe_emitted_spans: HashSet<usize>,
    /// Function activation markers already emitted into this main compiler.
    /// A definition compiled inside a top-level hard scope is also present in
    /// the source-ordered activation inventory; both paths must not emit the
    /// same `DefineEvalFunction` twice (Issue #11683).
    pub(super) emitted_eval_function_activations: HashSet<usize>,
    /// Captured variables from outer scope (for closures).
    /// When compiling a closure body, this contains the names of variables
    /// that were captured from the enclosing function scope.
    pub(super) captured_vars: HashSet<String>,
    /// Issue #8118: authoritative capture sets for the directly-nested functions
    /// of the body currently being compiled, keyed by qualified name
    /// (`parent#nested`). Populated by `prescan_mutual_closure_captures` BEFORE
    /// the body's statements compile. A non-empty entry means the nested
    /// function participates in a mutually-recursive closure group that captures
    /// an enclosing local; its `FunctionDef` uses this set verbatim (sibling
    /// function names excluded — those are reconstructed at the call site, not
    /// data-captured) instead of recomputing free variables.
    pub(super) mutual_closure_captures: std::collections::HashMap<String, HashSet<String>>,
    /// Current enclosing function name (for creating qualified nested function names).
    /// Used to disambiguate nested functions with the same name in different parent functions.
    pub(super) current_function_name: Option<String>,
    /// True while compiling Base/prelude function bodies. Unqualified calls inside
    /// Base must keep resolving to Base/builtin behavior even when a loaded module
    /// defines the same public name (e.g. MacroTools.match).
    pub(super) in_base_function_scope: bool,
    /// Top-level/module `const` bindings whose first assignment has completed.
    pub(super) const_bindings: HashSet<String>,
    /// `const` bindings declared immediately before their first assignment.
    pub(super) pending_const_bindings: HashSet<String>,
    /// Compile-time values for `const` bindings that fold to `ConstValue`.
    pub(super) const_values: HashMap<String, crate::compile::lattice::types::ConstValue>,
    /// True while compiling the operand of an `@inbounds` macro.
    pub(super) inbounds_context: bool,
    /// `(array_var, index_var)` pairs proven in-bounds by the current loop.
    pub(super) proven_inbounds_indices: Vec<(String, String)>,
    /// Names declared `global` in the current function scope (`global x`).
    ///
    /// Reads of these names resolve to the module-level (frame 0) binding and
    /// writes route there via `StoreGlobalAny`, instead of introducing a local
    /// binding in the function frame (Issues #5548, #5549). Populated by a
    /// pre-scan of the function body before statement compilation begins.
    pub(super) declared_globals: HashSet<String>,
    /// Depth of `try` bodies where a statically-known dispatch miss must still
    /// compile into a catchable runtime MethodError.
    pub(super) catchable_runtime_error_depth: usize,
}

/// Check if a ValueType is an integer type (signed or unsigned)
pub(super) fn is_integer_type(ty: &ValueType) -> bool {
    matches!(
        ty,
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
}

/// Check if a ValueType is a floating-point type
pub(super) fn is_float_type(ty: &ValueType) -> bool {
    matches!(ty, ValueType::F64 | ValueType::F32 | ValueType::F16)
}

/// Check if a ValueType is any numeric type (integer or float)
pub(super) fn is_numeric_type(ty: &ValueType) -> bool {
    is_integer_type(ty) || is_float_type(ty)
}

pub(super) fn static_assignment_types_compatible(target: &ValueType, incoming: &ValueType) -> bool {
    target == incoming
        || (is_numeric_type(target) && is_numeric_type(incoming))
        || matches!(
            (target, incoming),
            (ValueType::Array, ValueType::ArrayOf(_, _))
        )
        || matches!(
            (target, incoming),
            (ValueType::ArrayOf(_, _), ValueType::Array)
        )
}

/// Check if a ValueType is a singleton type.
///
/// Singleton value types whose equality (`==`) and identity (`===`) are
/// equivalent for the binary equality shortcut.
///
/// SINGLETON_HANDLING: When modifying identity ops, update equality ops too.
pub(super) fn is_singleton_type(ty: &ValueType) -> bool {
    matches!(ty, ValueType::Nothing | ValueType::Symbol | ValueType::Char)
}

impl<'a> CoreCompiler<'a> {
    pub(super) fn infer_shared_function_return_type_with_arg_types(
        &self,
        func: &Function,
        arg_value_types: &[ValueType],
    ) -> ValueType {
        if super::constants::is_generated_function(func) {
            return ValueType::Any;
        }

        // Issue #5425 / #5466: call-site refinement only sees positional
        // arguments. Returning an unannotated optional kwarg must therefore stay
        // dynamic so keyword calls can pass values with any runtime type.
        if crate::compile::returns_unannotated_optional_kwparam_value(func) {
            return ValueType::Any;
        }

        let mut engine = crate::compile::inference::build_shared_inference_engine(
            &self.shared_ctx.struct_table,
            &self.shared_ctx.global_types,
            std::iter::once(func),
        );
        // This single-function engine otherwise can't see other functions'
        // method tables, so a body call like `first(xs)` falls back to the
        // element-type tfunc — wrong when the user has overridden `first`/`last`/
        // `getindex` to return a non-element type, which then mis-coerced the
        // wrapper's result (Issue #6657). Seed only the tables that contain a
        // *user* override (cheap `Arc` clones) so the body re-inference resolves
        // those overrides to the method's declared return type; tables with only
        // Base methods are left out so the tfunc fast path (element-type
        // precision) stays intact for the common, non-overridden case.
        let user_overridden_tables = self.method_tables.iter().filter(|(_, table)| {
            table.methods.iter().any(|method| {
                self.shared_ctx
                    .function_ir_by_global_index
                    .contains_key(&method.global_index)
            })
        });
        engine.seed_initial_method_tables(user_overridden_tables);
        let arg_lattice_types: Vec<_> = arg_value_types
            .iter()
            .map(|vt| {
                crate::runtime_types::bridge::value_type_to_lattice_with_struct_table(
                    vt,
                    &self.shared_ctx.struct_table,
                )
            })
            .collect();
        let return_type = engine.infer_function_with_arg_types(func, &arg_lattice_types);
        crate::runtime_types::bridge::lattice_to_value_type(&return_type)
    }

    pub(super) fn should_accept_body_reinferred_call_return_type(
        &self,
        inferred: &ValueType,
    ) -> bool {
        // Issue #8414: body re-inference is an opportunistic refinement for
        // methods whose table metadata is `Any`. Do not let it narrow such a
        // call to the singleton `Nothing`: assignment code treats `Nothing` as
        // a no-slot value and drops the real runtime result. A method that
        // truly always returns `nothing` should already have `Nothing` metadata.
        !matches!(inferred, ValueType::Any | ValueType::Nothing)
    }

    pub(super) fn function_index_is_generated(&self, global_index: usize) -> bool {
        self.shared_ctx
            .function_ir_by_global_index
            .get(&global_index)
            .is_some_and(super::constants::is_generated_function)
    }

    /// Recovers the element type of `for var in iterable` when `iterable`'s
    /// static type is a user struct implementing the Base `iterate(struct,
    /// state)` protocol.
    ///
    /// `compile/stmt.rs`'s pure-Julia `Stmt::ForEach` lowering has no
    /// visibility into `iterate`'s inferred return type, so the loop
    /// variable always fell back to `ValueType::Any` — degrading every
    /// subsequent field access / arithmetic op on it to dynamic dispatch,
    /// even though upstream Julia infers a concrete element type (Issue
    /// #9124).
    ///
    /// Dispatches the arity-2 method directly with an explicit `Int64`
    /// state argument: the arity-1 forwarding overload
    /// `iterate(x) = iterate(x, 1)` cannot be re-inferred through the
    /// single-function engine below (its own body's nested `iterate(x, 1)`
    /// call cannot resolve without the arity-2 method's argument types
    /// already fixed), so re-inferring it directly would only recover a
    /// looser `Top`/`Any` result. The return type is then matched against
    /// `Union{Nothing, Tuple{T, State}}` (Julia's iterate-protocol
    /// contract) to extract `T`. Returns `None` for a non-struct iterable,
    /// no matching user `iterate` override, or any other return shape, so
    /// the caller keeps its existing `ValueType::Any` fallback.
    pub(super) fn infer_foreach_iterate_element_type(
        &self,
        iterable_ty: &JuliaType,
    ) -> Option<ValueType> {
        let JuliaType::Struct(struct_name) = iterable_ty else {
            return None;
        };
        let struct_info = self.shared_ctx.struct_table.get(struct_name)?;
        let julia_arg_types = [iterable_ty.clone(), JuliaType::Int64];

        let global_index = ["iterate", "Base.iterate"].iter().find_map(|name| {
            self.method_tables
                .get(*name)
                .and_then(|table| table.dispatch(&julia_arg_types).ok())
                .map(|method| method.global_index)
        })?;
        let func = self
            .shared_ctx
            .function_ir_by_global_index
            .get(&global_index)?;

        let mut engine = crate::compile::inference::build_shared_inference_engine(
            &self.shared_ctx.struct_table,
            &self.shared_ctx.global_types,
            std::iter::once(func),
        );
        // Issue #9124: the compile-time `ValueType` for an array-of-struct
        // field (e.g. `pts::Vector{Point}`) is intentionally the bare,
        // element-erased `ValueType::Array` — see `julia_type_to_value_type`'s
        // `VectorOf`/`MatrixOf` arm — because widening it to
        // `ArrayOf(StructOf(id))` would make comprehension-built
        // `Vector{Point}` values (which convert through `ArrayOf(Any)`) fail
        // the runtime's strict element-convert check. Enrich only *this*
        // scoped inference engine's struct-field lattice (never the
        // `ValueType` used for storage/convert) so that re-inferring
        // `iterate`'s body sees `pl.pts[state]` as `Point`, not `Any`.
        for def in &self.shared_ctx.struct_defs {
            for (field_name, field_jt) in def
                .fields
                .iter()
                .map(|(name, _)| name)
                .zip(def.field_julia_types.iter())
            {
                if let Some(elem) =
                    array_of_struct_element_concrete(field_jt, &self.shared_ctx.struct_table)
                {
                    engine.set_struct_field_type(
                        &def.name,
                        field_name,
                        crate::compile::lattice::types::LatticeType::Concrete(
                            crate::compile::lattice::types::ConcreteType::Array {
                                element: Box::new(elem),
                                ndims: None,
                            },
                        ),
                    );
                }
            }
        }
        let arg_lattice_types = [
            crate::runtime_types::bridge::value_type_to_lattice_with_struct_table(
                &ValueType::Struct(struct_info.type_id),
                &self.shared_ctx.struct_table,
            ),
            crate::runtime_types::bridge::value_type_to_lattice_with_struct_table(
                &ValueType::I64,
                &self.shared_ctx.struct_table,
            ),
        ];
        let return_ty = engine.infer_function_with_arg_types(func, &arg_lattice_types);
        let elem_lattice =
            crate::compile::abstract_interp::loop_analysis::iterate_return_element_lattice(
                &return_ty,
            )?;
        let elem_value_type = crate::runtime_types::bridge::lattice_to_value_type(&elem_lattice);
        (elem_value_type != ValueType::Any).then_some(elem_value_type)
    }

    pub(super) fn new(
        method_tables: &'a HashMap<String, MethodTable>,
        module_functions: &'a HashMap<String, HashSet<String>>,
        module_exports: &'a HashMap<String, HashSet<String>>,
        imported_functions: &'a HashSet<String>,
        usings: &'a HashSet<String>,
        resolved_usings: Vec<ResolvedUsingImport>,
        shared_ctx: &'a mut SharedCompileContext,
        abstract_type_names: &'a HashSet<String>,
        module_constants: &'a HashMap<String, HashSet<String>>,
    ) -> Self {
        // Imported bindings activate at executable source positions. Keep the
        // legacy/static maps empty at compiler construction; assignment aliases
        // populate them as statements execute, while imports use the runtime
        // activation state (Issues #11203/#11216).
        let module_alias_states = HashMap::new();
        let module_aliases = HashMap::new();
        let mut import_alias_assignments: HashMap<String, Vec<(String, String, Span)>> =
            HashMap::new();
        for (alias, lowered_source, canonical_source, span) in resolved_usings
            .iter()
            .flat_map(|using| using.alias_assignments.iter().cloned())
        {
            import_alias_assignments.entry(alias).or_default().push((
                lowered_source,
                canonical_source,
                span,
            ));
        }
        let renamed_only_module_roots = resolved_usings
            .iter()
            .filter(|using| {
                !using.binds_module_root
                    && using.has_renames
                    && (module_functions.contains_key(&using.module)
                        || module_exports.contains_key(&using.module)
                        || crate::module_names::is_language_root(&using.module))
            })
            .filter_map(|using| using.module.rsplit('.').next().map(str::to_string))
            .collect();
        let imported_bindings = imported_binding_names(
            module_functions,
            module_exports,
            module_constants,
            &resolved_usings,
        );
        Self {
            code: Vec::with_capacity(64),
            source_map: Vec::with_capacity(64),
            current_span: None,
            locals: HashMap::new(),
            initialized_locals: HashSet::new(),
            julia_type_locals: HashMap::new(),
            known_any_rank_array_locals: HashSet::new(),
            function_aliases: HashMap::new(),
            lexical_function_tables: HashMap::new(),
            type_value_aliases: HashMap::new(),
            method_tables,
            module_functions,
            module_exports,
            imported_functions,
            hidden_user_globals: HashSet::new(),
            usings,
            resolved_usings,
            toplevel_module_bindings: HashSet::new(),
            preexisting_global_bindings: HashSet::new(),
            import_alias_assignments,
            renamed_only_module_roots,
            shared_ctx,
            temp_counter: 0,
            loop_stack: Vec::new(),
            finally_stack: Vec::new(),
            scope_cleanup_stack: Vec::new(),
            lexical_scope_locals: HashSet::new(),
            explicit_lexical_scopes: false,
            explicit_lexical_scope_stack: Vec::new(),
            strict_undefined_check: false, // Default to lenient for module/main
            repl_source_ordered_top_level_dispatch: false,
            repl_source_ordered_type_names: HashSet::new(),
            local_scope_depth: 0,
            current_struct_type_id: None,
            current_parametric_struct_name: None,
            any_params: HashSet::new(), // No params in module/main context
            abstract_numeric_params: HashSet::new(),
            module_aliases,
            module_alias_states,
            imported_bindings,
            active_imported_bindings: HashMap::new(),
            using_import_cursor: 0,
            abstract_type_names,
            current_type_params: Vec::new(),
            current_type_param_index: HashMap::new(),
            ctor_arg_bound_type_vars: HashSet::new(),
            ctor_self_bound_type_vars: HashSet::new(),
            mixed_type_vars: HashSet::new(), // No mixed type vars in module/main context
            val_type_params: HashSet::new(),
            val_bool_params: HashSet::new(),
            val_symbol_params: HashSet::new(),
            current_module_path: None,
            current_module_is_bare: false,
            current_module_imports: HashSet::new(),
            module_constants,
            label_positions: HashMap::new(),
            goto_patches: Vec::new(),
            captured_vars: HashSet::new(),
            mutual_closure_captures: std::collections::HashMap::new(),
            current_function_name: None,
            in_base_function_scope: false,
            const_bindings: HashSet::new(),
            pending_const_bindings: HashSet::new(),
            const_values: HashMap::new(),
            inbounds_context: false,
            proven_inbounds_indices: Vec::new(),
            declared_globals: HashSet::new(),
            catchable_runtime_error_depth: 0,
            where_probe_emitted_spans: HashSet::new(),
            emitted_eval_function_activations: HashSet::new(),
        }
    }

    pub(super) fn new_for_function(
        method_tables: &'a HashMap<String, MethodTable>,
        module_functions: &'a HashMap<String, HashSet<String>>,
        module_exports: &'a HashMap<String, HashSet<String>>,
        imported_functions: &'a HashSet<String>,
        usings: &'a HashSet<String>,
        resolved_usings: Vec<ResolvedUsingImport>,
        shared_ctx: &'a mut SharedCompileContext,
        abstract_type_names: &'a HashSet<String>,
        module_constants: &'a HashMap<String, HashSet<String>>,
    ) -> Self {
        // Function bodies are compiled once but can run after any source-order
        // import transition. Keep the final lexical table for static type
        // identity and first-explicit-binding precedence; runtime imported
        // value/module reads still take the hidden activation path first.
        let module_alias_states = build_canonical_module_alias_states(
            &resolved_usings,
            module_functions,
            module_exports,
            &shared_ctx.module_imported_bindings,
        );
        let module_aliases = bound_module_aliases(&module_alias_states);
        let mut import_alias_assignments: HashMap<String, Vec<(String, String, Span)>> =
            HashMap::new();
        for (alias, lowered_source, canonical_source, span) in resolved_usings
            .iter()
            .flat_map(|using| using.alias_assignments.iter().cloned())
        {
            import_alias_assignments.entry(alias).or_default().push((
                lowered_source,
                canonical_source,
                span,
            ));
        }
        let renamed_only_module_roots = resolved_usings
            .iter()
            .filter(|using| {
                !using.binds_module_root
                    && using.has_renames
                    && (module_functions.contains_key(&using.module)
                        || module_exports.contains_key(&using.module)
                        || crate::module_names::is_language_root(&using.module))
            })
            .filter_map(|using| using.module.rsplit('.').next().map(str::to_string))
            .collect();
        let imported_bindings = imported_binding_names(
            module_functions,
            module_exports,
            module_constants,
            &resolved_usings,
        );
        Self {
            code: Vec::with_capacity(64),
            source_map: Vec::with_capacity(64),
            current_span: None,
            locals: HashMap::new(),
            initialized_locals: HashSet::new(),
            julia_type_locals: HashMap::new(),
            known_any_rank_array_locals: HashSet::new(),
            function_aliases: HashMap::new(),
            lexical_function_tables: HashMap::new(),
            type_value_aliases: HashMap::new(),
            method_tables,
            module_functions,
            module_exports,
            imported_functions,
            hidden_user_globals: HashSet::new(),
            usings,
            resolved_usings,
            toplevel_module_bindings: HashSet::new(),
            preexisting_global_bindings: HashSet::new(),
            import_alias_assignments,
            renamed_only_module_roots,
            shared_ctx,
            temp_counter: 0,
            loop_stack: Vec::new(),
            finally_stack: Vec::new(),
            scope_cleanup_stack: Vec::new(),
            lexical_scope_locals: HashSet::new(),
            explicit_lexical_scopes: false,
            explicit_lexical_scope_stack: Vec::new(),
            strict_undefined_check: true, // Strict for function bodies
            repl_source_ordered_top_level_dispatch: false,
            repl_source_ordered_type_names: HashSet::new(),
            local_scope_depth: 0,
            any_params: HashSet::new(), // Will be populated after creation
            abstract_numeric_params: HashSet::new(), // Will be populated after creation
            module_aliases,
            module_alias_states,
            imported_bindings,
            active_imported_bindings: HashMap::new(),
            using_import_cursor: 0,
            abstract_type_names,
            current_struct_type_id: None,
            current_parametric_struct_name: None,
            current_type_params: Vec::new(), // Will be set after creation
            current_type_param_index: HashMap::new(), // Will be set after creation
            ctor_arg_bound_type_vars: HashSet::new(), // Will be set after creation
            ctor_self_bound_type_vars: HashSet::new(),
            mixed_type_vars: HashSet::new(), // Will be populated from type inference
            val_type_params: HashSet::new(), // Will be populated from parameter analysis
            val_bool_params: HashSet::new(), // Will be populated from parameter analysis
            val_symbol_params: HashSet::new(), // Will be populated from parameter analysis
            current_module_path: None,       // Will be set after creation
            current_module_is_bare: false,
            current_module_imports: HashSet::new(), // Will be set after creation
            module_constants,
            label_positions: HashMap::new(),
            goto_patches: Vec::new(),
            captured_vars: HashSet::new(), // Will be populated for closures
            mutual_closure_captures: std::collections::HashMap::new(),
            current_function_name: None, // Will be set when compiling functions
            in_base_function_scope: false, // Will be set when compiling functions
            const_bindings: HashSet::new(),
            pending_const_bindings: HashSet::new(),
            const_values: HashMap::new(),
            inbounds_context: false,
            proven_inbounds_indices: Vec::new(),
            declared_globals: HashSet::new(),
            catchable_runtime_error_depth: 0,
            where_probe_emitted_spans: HashSet::new(),
            emitted_eval_function_activations: HashSet::new(),
        }
    }

    pub(super) fn resolve_function_alias_value(
        &self,
        expr: &crate::ir::core::Expr,
    ) -> Option<String> {
        match expr {
            crate::ir::core::Expr::Var(name, _) => {
                if self.explicit_lexical_owner_active(name) {
                    return self.function_aliases.get(name.as_str()).cloned();
                }
                if (self.method_tables.contains_key(name.as_str())
                    && !self.hidden_user_globals.contains(name.as_str()))
                    || super::is_base_function(name)
                {
                    Some(name.to_string())
                } else {
                    self.function_aliases.get(name.as_str()).cloned()
                }
            }
            crate::ir::core::Expr::FunctionRef { name, .. } => Some(name.to_string()),
            _ => None,
        }
    }

    pub(super) fn push_proven_inbounds_index(&mut self, array_var: &str, index_var: &str) {
        self.proven_inbounds_indices
            .push((array_var.to_string(), index_var.to_string()));
    }

    pub(super) fn pop_proven_inbounds_index(&mut self) {
        self.proven_inbounds_indices.pop();
    }

    pub(super) fn enter_catchable_runtime_error_region(&mut self) {
        self.catchable_runtime_error_depth += 1;
    }

    pub(super) fn exit_catchable_runtime_error_region(&mut self) {
        debug_assert!(self.catchable_runtime_error_depth > 0);
        self.catchable_runtime_error_depth = self.catchable_runtime_error_depth.saturating_sub(1);
    }

    pub(super) fn in_catchable_runtime_error_region(&self) -> bool {
        self.catchable_runtime_error_depth > 0
    }

    pub(super) fn is_proven_inbounds_index(
        &self,
        array: &crate::ir::core::Expr,
        index: &crate::ir::core::Expr,
    ) -> bool {
        let crate::ir::core::Expr::Var(array_var, _) = array else {
            return false;
        };
        let crate::ir::core::Expr::Var(index_var, _) = index else {
            return false;
        };
        self.proven_inbounds_indices
            .iter()
            .rev()
            .any(|(arr, idx)| arr == array_var && idx == index_var)
    }

    pub(super) fn resolve_static_datatype_value(
        &self,
        expr: &crate::ir::core::Expr,
    ) -> Option<JuliaType> {
        match expr {
            crate::ir::core::Expr::Builtin {
                name: crate::ir::core::BuiltinOp::TypeOf,
                args,
                ..
            } => match args.first() {
                Some(crate::ir::core::Expr::Literal(
                    crate::ir::core::Literal::Str(type_name),
                    _,
                )) => Some(
                    JuliaType::from_name(type_name)
                        .unwrap_or_else(|| JuliaType::from_name_or_struct(type_name)),
                ),
                _ => None,
            },
            crate::ir::core::Expr::Literal(crate::ir::core::Literal::DataType(type_name), _) => {
                Some(
                    JuliaType::from_name(type_name)
                        .unwrap_or_else(|| JuliaType::from_name_or_struct(type_name)),
                )
            }
            crate::ir::core::Expr::Var(name, _) => {
                let lexical_alias = self.type_value_aliases.get(name.as_str()).cloned();
                if lexical_alias.is_some() || self.explicit_lexical_owner_active(name) {
                    lexical_alias
                } else {
                    self.resolved_active_imported_type_name(name)
                        .map(|type_name| {
                            JuliaType::from_name(&type_name)
                                .unwrap_or_else(|| JuliaType::from_name_or_struct(&type_name))
                        })
                }
            }
            _ => None,
        }
    }

    /// Convert JuliaType to ValueType, resolving struct types using the struct table.
    pub(super) fn julia_type_to_value_type_with_ctx(&self, jt: &JuliaType) -> ValueType {
        match jt {
            JuliaType::Bottom => ValueType::Union(Vec::new()),
            JuliaType::Union(types) => ValueType::Union(
                types
                    .iter()
                    .map(|ty| self.julia_type_to_value_type_with_ctx(ty))
                    .collect::<Vec<_>>(),
            ),
            JuliaType::Struct(name) => {
                // RNG type annotations (Xoshiro/StableRNG/MersenneTwister/
                // TaskLocalRNG/AbstractRNG) map to ValueType::Rng so randn(rng) /
                // rand(rng) on such a param compile to the scalar-from-rng form
                // (Issue #7231).
                if super::type_helpers::is_rng_type_name(name) {
                    return ValueType::Rng;
                }
                if let Some(memory_type) =
                    super::type_helpers::memory_struct_name_to_value_type(name)
                {
                    return memory_type;
                }
                // Look up type_id from struct_table, origin-aware so a
                // `p::Partition` annotation inside Base's OWN function body
                // resolves to Base's struct even if a `using`-imported
                // module's same-named bare alias later clobbered the live
                // `struct_table` entry (Issue #10078).
                if let Some(info) = self.resolve_struct_info_scoped(name) {
                    ValueType::Struct(info.type_id)
                } else {
                    // A missed complete name cannot be replaced with the bare
                    // family or an arbitrary registered sibling. Runtime
                    // specialization resolves the concrete value (Issue #11436).
                    ValueType::Any
                }
            }
            // Issue #9133: `a::Vector{T}` / `a::Matrix{T}` parameter
            // annotations keep their element type. The generic
            // `julia_type_to_value_type` widens both to `ValueType::Array`,
            // which loses the element type the annotation just declared —
            // `a[i]` inferred unknown, loop accumulators widened to `Any`,
            // and the typed function compiled to MORE dynamic dispatch than
            // its un-annotated twin. Mirrors the `Memory{T}` parameter
            // handling from Issue #9009 (`memory_struct_name_to_value_type`
            // above). Element types the array runtime cannot represent fall
            // back to the plain `Array` carrier.
            JuliaType::VectorOf(_) | JuliaType::MatrixOf(_) => {
                let ndims = match jt {
                    JuliaType::VectorOf(_) => Some(1),
                    _ => Some(2),
                };
                match self.array_julia_type_element_type(jt) {
                    Some(elem) => ValueType::ArrayOf(elem, ndims),
                    None => ValueType::Array,
                }
            }
            _ => julia_type_to_value_type(jt),
        }
    }

    /// Check if a ValueType represents a struct with the given base name
    pub(super) fn is_struct_type_of(&self, ty: ValueType, base_name: &str) -> bool {
        self.shared_ctx.is_struct_type_of(&ty, base_name)
    }

    /// Get any type_id for a struct with the given base name
    pub(super) fn get_struct_type_id(&self, base_name: &str) -> Option<usize> {
        self.shared_ctx.get_struct_type_id(base_name)
    }

    /// Find the capture set for a callable as resolved from the current lexical
    /// scope. Nested functions are registered as `parent#child`, while module
    /// body callables use `Module.child`; a bare lookup alone can therefore
    /// mistake an unknown local capture set for a capture-free global.
    pub(super) fn scoped_closure_captures(
        &self,
        name: &str,
    ) -> Option<(&String, &HashSet<String>)> {
        if name.contains('#') || name.contains('.') {
            return self.shared_ctx.closure_captures.get_key_value(name);
        }

        if let Some(current) = &self.current_function_name {
            let segments: Vec<&str> = current.split('#').collect();
            for depth in (1..=segments.len()).rev() {
                let qualified = format!("{}#{name}", segments[..depth].join("#"));
                if let Some(entry) = self.shared_ctx.closure_captures.get_key_value(&qualified) {
                    return Some(entry);
                }
            }
        }
        if let Some(module_path) = &self.current_module_path {
            let qualified = format!("{module_path}.{name}");
            if let Some(entry) = self.shared_ctx.closure_captures.get_key_value(&qualified) {
                return Some(entry);
            }
        }

        self.shared_ctx.closure_captures.get_key_value(name)
    }

    /// Resolve a struct spelling through the canonical owner/scope authority
    /// (Issue #11046). The registry preserves shadowed declarations itself, so
    /// Base/prelude bodies no longer need a parallel alias table.
    pub(super) fn resolve_struct_info_scoped<'b>(
        &'b self,
        name: &str,
    ) -> Option<&'b super::context::StructInfo> {
        self.shared_ctx
            .struct_table
            .resolve_scoped(
                name,
                self.current_module_path.as_deref(),
                self.in_base_function_scope,
            )
            .map(|(_, info)| info)
    }

    /// Resolve a struct name, trying both qualified and unqualified versions.
    /// When inside a module (e.g., Dates), prefer the qualified name (e.g., "Dates.Month")
    /// over the unqualified name ("Month") for method dispatch to work correctly.
    pub(super) fn resolve_struct_name(&self, name: &str) -> Option<String> {
        // An explicit qualification is already a complete nominal identity.
        // Do not suffix-remap `M.T` to a nested `Outer.M.T` declaration.
        if name.contains('.') && self.shared_ctx.struct_table.contains_key(name) {
            return Some(name.to_string());
        }

        // If inside a module, prefer qualified name first
        if let Some(ref module_path) = self.current_module_path {
            let qualified = format!("{}.{}", module_path, name);
            if self.shared_ctx.struct_table.contains_key(&qualified) {
                return Some(qualified);
            }
        }

        // Try exact name (unqualified or already qualified)
        if self.shared_ctx.struct_table.contains_key(name) {
            // Check if there's a qualified version (module struct imported with short name)
            // For correct method dispatch, we need to use the qualified name (e.g., "Dates.Day")
            // even when called from outside the module via `using Dates`
            for key in self.shared_ctx.struct_table.keys() {
                if key.ends_with(&format!(".{}", name)) && key != name {
                    // Found qualified version - use it for correct method dispatch
                    return Some(key.clone());
                }
            }
            return Some(name.to_string());
        }

        // Not found
        None
    }

    pub(super) fn resolve_parametric_struct_name(&self, name: &str) -> Option<String> {
        if let Some(unqualified) = name.strip_prefix("Base.") {
            if self.shared_ctx.parametric_structs.contains_key(unqualified) {
                return Some(unqualified.to_string());
            }
        }

        // An explicit qualification is already a complete nominal identity.
        // Keep it ahead of every short-name/suffix fallback (Issue #11076).
        if name.contains('.') && self.shared_ctx.parametric_structs.contains_key(name) {
            return Some(name.to_string());
        }

        // Scope-aware resolution (Issue #8313): a bare name must resolve to a
        // struct that is actually in scope — defined in the current module, or
        // brought in by `using` — rather than an arbitrary same-named struct
        // elsewhere. Without this, a user `Perm` (from `using .M`) collides with
        // the bundled `Base.Order.Perm` the program never imported, and the
        // suffix-match fallbacks below (which iterate a `HashMap`) pick one
        // nondeterministically — `Perm([1,2,3])` then sometimes resolves to the
        // 2-parameter `Order.Perm`. Mirrors `resolve_visible_type_alias`.
        if let Some(module_path) = &self.current_module_path {
            let qualified = format!("{}.{}", module_path, name);
            if self.shared_ctx.parametric_structs.contains_key(&qualified) {
                return Some(qualified);
            }
        }
        for using_module in self.visible_using_modules_for_name(name) {
            let qualified = format!("{}.{}", using_module, name);
            if self.shared_ctx.parametric_structs.contains_key(&qualified) {
                return Some(qualified);
            }
        }

        // An exact top-level binding wins after current-module and explicit
        // `using` resolution.  Do not replace it with an arbitrary qualified
        // suffix match: if `M.X` was registered before a later top-level `X`,
        // bare `X{T}` must denote the top-level type regardless of definition
        // order (Issue #10959).
        if self.shared_ctx.parametric_structs.contains_key(name) {
            return Some(name.to_string());
        }

        // Also search for qualified names even when the unqualified name doesn't
        // exist (e.g. when only the qualified name is registered). Deterministic.
        self.first_qualified_struct_key(name)
    }

    /// Smallest (deterministic) `Module.name` key among registered parametric
    /// structs whose qualified name ends in `.name`. Used as the out-of-scope
    /// fallback for `resolve_parametric_struct_name`; the scope-aware lookups run
    /// first, so this only disambiguates names with no in-scope binding, where any
    /// stable choice beats `HashMap`-order nondeterminism (Issue #8313).
    fn first_qualified_struct_key(&self, name: &str) -> Option<String> {
        let suffix = format!(".{}", name);
        self.shared_ctx
            .parametric_structs
            .keys()
            .filter(|key| key.ends_with(&suffix) && key.as_str() != name)
            .min()
            .cloned()
    }

    pub(super) fn enable_explicit_lexical_scopes(&mut self) {
        self.explicit_lexical_scopes = true;
    }

    pub(super) fn enter_explicit_lexical_scope(&mut self, mut names: Vec<String>) -> bool {
        if !self.explicit_lexical_scopes || names.is_empty() {
            return false;
        }
        names.sort();
        names.dedup();
        self.emit(Instr::EnterLexicalScope(names.clone()));
        let mut hidden_aliases = Vec::with_capacity(names.len());
        for name in &names {
            hidden_aliases.push(ExplicitLexicalAliasState {
                name: name.clone(),
                function_alias: self.function_aliases.remove(name),
                lexical_function_table: self.lexical_function_tables.remove(name),
                type_value_alias: self.type_value_aliases.remove(name),
                module_alias: self.module_aliases.remove(name),
                inherited_declared_global: self.declared_globals.remove(name),
            });
        }
        self.explicit_lexical_scope_stack
            .push(ExplicitLexicalScopeState {
                names: names.into_iter().collect(),
                hidden_aliases,
            });
        true
    }

    pub(super) fn exit_explicit_lexical_scope(&mut self) {
        debug_assert!(
            !self.explicit_lexical_scope_stack.is_empty(),
            "explicit lexical scope exit must have a matching enter"
        );
        let Some(scope) = self.explicit_lexical_scope_stack.pop() else {
            return;
        };
        for alias in scope.hidden_aliases {
            self.function_aliases.remove(&alias.name);
            self.lexical_function_tables.remove(&alias.name);
            self.type_value_aliases.remove(&alias.name);
            self.module_aliases.remove(&alias.name);
            self.declared_globals.remove(&alias.name);
            if let Some(value) = alias.function_alias {
                self.function_aliases.insert(alias.name.clone(), value);
            }
            if let Some(value) = alias.lexical_function_table {
                self.lexical_function_tables
                    .insert(alias.name.clone(), value);
            }
            if let Some(value) = alias.type_value_alias {
                self.type_value_aliases.insert(alias.name.clone(), value);
            }
            if let Some(value) = alias.module_alias {
                self.module_aliases.insert(alias.name.clone(), value);
            }
            if alias.inherited_declared_global {
                self.declared_globals.insert(alias.name);
            }
        }
        self.emit(Instr::ExitLexicalScope);
    }

    pub(super) fn explicit_lexical_owner_active(&self, name: &str) -> bool {
        self.explicit_lexical_scopes
            && self
                .explicit_lexical_scope_stack
                .iter()
                .rev()
                .any(|scope| scope.names.contains(name))
            && !self.declared_globals.contains(name)
    }

    pub(super) fn emit(&mut self, i: Instr) {
        // Direct name-based loads/stores are emitted throughout lowering (not
        // only via `load_local`/`store_local`). Central routing is therefore
        // the authority that keeps every access to an active hard-scope owner
        // out of frame zero (Issues #11569/#9784).
        let i = match i {
            Instr::LoadStr(name)
            | Instr::LoadI64(name)
            | Instr::LoadF64(name)
            | Instr::LoadF32(name)
            | Instr::LoadF16(name)
            | Instr::LoadBool(name)
            | Instr::LoadAny(name)
            | Instr::LoadArray(name)
            | Instr::LoadRange(name)
            | Instr::LoadStruct(name)
            | Instr::LoadRng(name)
            | Instr::LoadTuple(name)
            | Instr::LoadNamedTuple(name)
            | Instr::LoadDict(name)
            | Instr::LoadSet(name)
            | Instr::LoadMemory(name)
                if self.explicit_lexical_owner_active(&name) =>
            {
                Instr::LoadLexical(name)
            }
            Instr::StoreStr(name)
            | Instr::StoreI64(name)
            | Instr::StoreF64(name)
            | Instr::StoreF32(name)
            | Instr::StoreF16(name)
            | Instr::StoreBool(name)
            | Instr::StoreAny(name)
            | Instr::StoreArray(name)
            | Instr::StoreRange(name)
            | Instr::StoreStruct(name)
            | Instr::StoreRng(name)
            | Instr::StoreTuple(name)
            | Instr::StoreNamedTuple(name)
            | Instr::StoreDict(name)
            | Instr::StoreSet(name)
            | Instr::StoreMemory(name)
                if self.explicit_lexical_owner_active(&name) =>
            {
                Instr::StoreLexical(name)
            }
            Instr::IsDefined(name) if self.explicit_lexical_owner_active(&name) => {
                Instr::IsLexicalDefined(name)
            }
            other => other,
        };
        self.code.push(i);
        self.source_map.push(self.current_span);
    }

    pub(super) fn set_current_span(&mut self, span: Span) {
        self.current_span = Some(span);
    }

    pub(super) fn current_span(&self) -> Option<Span> {
        self.current_span
    }

    pub(super) fn emit_function_value(&mut self, name: &str) {
        self.emit_function_value_named(name, name);
    }

    /// Emit closure construction with the exact callable family when the
    /// compiler can resolve it. Keeping the indices on the value prevents a
    /// later same-named private helper/source method from changing the closure's
    /// identity; unresolved legacy paths retain name lookup (Issue #9784).
    pub(super) fn emit_closure_value(
        &mut self,
        name: &str,
        capture_names: Vec<String>,
        candidate_indices: Vec<usize>,
    ) {
        if candidate_indices.is_empty() {
            self.emit(Instr::CreateClosure {
                func_name: name.to_string(),
                capture_names,
            });
        } else {
            self.emit(Instr::CreateResolvedClosure(Box::new(
                crate::bytecode::ResolvedClosureOperands {
                    name: name.to_string(),
                    capture_names,
                    candidate_indices,
                },
            )));
        }
    }

    /// Emit a function-value reference whose runtime type identity
    /// (`typeof(f)` / `isa Function`) uses `display_name` when it is safe to,
    /// while resolving candidate method indices through the (possibly
    /// module-qualified) `lookup_name` method-table key.
    ///
    /// Module-scoped functions are registered in `method_tables` under a
    /// module-qualified key (`"Pkg9992B.transform9992b"`, `"Base.sqrt"`) to
    /// disambiguate same-named functions across modules — but that internal
    /// key is not the function's identity. Upstream Julia's generic function
    /// has ONE canonical name (`nameof(f)`) regardless of the access path
    /// used to reach it: `Module.func` (module-qualified) and `func`
    /// (bare/imported) capture the exact same value, with the exact same
    /// `typeof`/`isa Function` result (Issue #10077). Baking the qualified
    /// lookup key into the captured `FunctionValue.name` instead of the bare
    /// declared name made the qualified-access path diverge from the bare
    /// path on both counts. Passing the resolved candidate indices alongside
    /// the identity name (whichever spelling is chosen, see below) preserves
    /// exact calling correctness regardless — bare-name collisions across
    /// modules never affect dispatch (e.g. `Base.Iterators.flatten` vs
    /// `MacroTools.flatten`, the reason `FunctionValue::candidate_indices`
    /// exists), only the runtime type identity that follows.
    ///
    /// That fix over-applied, though: it always used the bare display name,
    /// which also collapses the runtime type identity of two DIFFERENT
    /// declarations from sibling modules that merely happen to share a bare
    /// name (Issue #11088 — the function-value analog of Issue #11021's
    /// same-named-struct collapse). `pipeline_ctx.rs`'s function
    /// registration unconditionally adds every module-scoped function under
    /// its own qualified key (`"M1x.f"`, `"M2x.f"`) in `method_tables`
    /// REGARDLESS of any `using` (Issue #11089's own root cause), so a naive
    /// "does another qualified key ending in this bare name exist anywhere"
    /// check is too broad: it also fires for an unrelated module that
    /// merely happens to declare the same bare name but was never brought
    /// into scope, which would wrongly diverge the identity of a `using`d
    /// declaration's bare and qualified access paths from each other
    /// (regressing Issue #10077's own invariant — caught by adversarial
    /// review before landing, MWE: a `using`d `A.f` plus an unrelated,
    /// never-`using`d `B.f`).
    ///
    /// The fix resolves the declaration's "owning module" symmetrically for
    /// BOTH access paths — directly from `lookup_name` when it is already
    /// qualified, or via [`Self::unique_using_owner`] when `display_name ==
    /// lookup_name` (bare) — and only distinguishes two declarations when
    /// `display_name` does NOT uniquely resolve (through an actual `using`)
    /// back to that same owner: an unrelated, non-`using`d sibling sharing
    /// the bare name never satisfies that uniqueness check, so it cannot
    /// flip the identity of a genuinely `using`-imported declaration, while
    /// two directly-qualified sibling declarations with no `using` in play
    /// at all (Issue #11088's own MWE) still fall through to the qualified
    /// spelling, keeping them distinct. This does not need a new
    /// `FunctionId` type (Issue #10990).
    pub(super) fn emit_function_value_named(&mut self, display_name: &str, lookup_name: &str) {
        let candidate_indices = self.imported_generic_candidate_indices(lookup_name);

        if candidate_indices.is_empty() {
            // No resolved method-table entry for `lookup_name`: fall back to
            // the original (possibly qualified) spelling so name-based
            // resolution at call time still finds the function under the
            // registry key it is actually stored at.
            self.emit(Instr::PushFunction(lookup_name.to_string()));
        } else {
            let identity_name = self.function_value_identity_name(display_name, lookup_name);
            self.emit(Instr::PushResolvedFunction(Box::new(
                ResolvedFunctionOperands {
                    name: identity_name,
                    candidate_indices,
                },
            )));
        }
    }

    /// The single `using`d module that declares `bare_name`, if exactly one
    /// does (Issue #11088). Returns `None` when no currently-`using`d module
    /// declares it (including "nothing is `using`d at all", the common case
    /// for a purely qualified access with no `using` in play) or when more
    /// than one does (a genuinely ambiguous export sjulia does not yet
    /// diagnose, Issue #11089) — both cases fall back to the caller's
    /// existing sibling-qualified-key check instead of a using-derived
    /// owner.
    ///
    /// `using M` only brings `M`'s *exported* names into unqualified scope
    /// (upstream `using` semantics) — a module that merely *defines*
    /// `bare_name` without exporting it is not a real candidate owner, even
    /// though it is `using`d for some other name. Mirrors the same
    /// `module_exports` visibility check the module-alias state builder above
    /// uses for the analogous "is this name visible via using"
    /// question: no recorded export set (or an empty one) means the module
    /// has no explicit `export` list, treated as exporting everything it
    /// defines; a non-empty set gates on membership.
    pub(super) fn unique_using_owner(&self, bare_name: &str) -> Option<&str> {
        let mut matches = self.usings.iter().filter(|module_path| {
            let Some(functions) = self.module_functions.get(module_path.as_str()) else {
                return false;
            };
            if !functions.contains(bare_name) {
                return false;
            }
            let exports = self.module_exports.get(module_path.as_str());
            exports.is_none_or(|e| e.is_empty() || e.contains(bare_name))
        });
        let first = matches.next()?;
        if matches.next().is_some() {
            return None;
        }
        Some(first.as_str())
    }

    /// Emit a function call instruction, choosing between Call and CallSpecialize.
    /// If the function has a specialization entry (needs_specialization was true),
    /// emit CallSpecialize to enable Lazy AoT compilation.
    pub(super) fn emit_call_or_specialize(
        &mut self,
        _func_name: &str,
        func_index: usize,
        arg_count: usize,
    ) {
        if let Some(&spec_idx) = self.shared_ctx.spec_func_mapping.get(&func_index) {
            if self.inbounds_context {
                self.emit(Instr::CallSpecializeInbounds(spec_idx, arg_count));
            } else {
                self.emit(Instr::CallSpecialize(spec_idx, arg_count));
            }
        } else if self.inbounds_context {
            self.emit(Instr::CallInbounds(func_index, arg_count));
        } else {
            self.emit(Instr::CallResolved(func_index, arg_count));
        }
    }

    /// Emit runtime method dispatch while retaining the compiler-resolved
    /// method-table identity for the shared call resolver (Issue #10461).
    pub(in crate::compile) fn emit_dynamic_call(
        &mut self,
        callee_name: &str,
        fallback_func_index: usize,
        arg_count: usize,
        candidates: Vec<DynamicCallCandidate>,
    ) {
        self.emit(Instr::call_dynamic(
            callee_name,
            fallback_func_index,
            arg_count,
            candidates,
        ));
    }

    pub(super) fn here(&self) -> usize {
        self.code.len()
    }

    pub(super) fn patch_jump(&mut self, at: usize, target: usize) {
        self.code[at] = match &self.code[at] {
            Instr::Jump(_) => Instr::Jump(target),
            Instr::JumpIfZero(_) => Instr::JumpIfZero(target),
            // Directional comparison jumps are emitted as exit tests by the
            // constant-step `for` loop fast path (Issue #5166).
            Instr::JumpIfLtI64(_) => Instr::JumpIfLtI64(target),
            Instr::JumpIfGtI64(_) => Instr::JumpIfGtI64(target),
            Instr::JumpIfGtI64Slots(lhs_slot, rhs_slot, _) => {
                Instr::JumpIfGtI64Slots(*lhs_slot, *rhs_slot, target)
            }
            Instr::AddConstI64SlotAndJumpIfLe(slot, delta, stop_slot, _) => {
                Instr::AddConstI64SlotAndJumpIfLe(*slot, *delta, *stop_slot, target)
            }
            Instr::JumpIfLeI64(_) => Instr::JumpIfLeI64(target),
            Instr::JumpIfGeI64(_) => Instr::JumpIfGeI64(target),
            Instr::JumpIfEqI64(_) => Instr::JumpIfEqI64(target),
            Instr::JumpIfNeI64(_) => Instr::JumpIfNeI64(target),
            Instr::JumpIfEqF64(_) => Instr::JumpIfEqF64(target),
            Instr::JumpIfNeF64(_) => Instr::JumpIfNeF64(target),
            Instr::JumpIfNotLtF64(_) => Instr::JumpIfNotLtF64(target),
            Instr::JumpIfNotGtF64(_) => Instr::JumpIfNotGtF64(target),
            Instr::JumpIfNotLeF64(_) => Instr::JumpIfNotLeF64(target),
            Instr::JumpIfNotGeF64(_) => Instr::JumpIfNotGeF64(target),
            _ => return,
        };
    }

    /// Patch all @goto jumps with the corresponding @label positions.
    /// This must be called after all statements have been compiled.
    /// Returns an error if any @goto references an undefined label.
    pub(super) fn patch_goto_jumps(&mut self) -> CResult<()> {
        for (patch_pos, label_name) in &self.goto_patches {
            if let Some(&label_pos) = self.label_positions.get(label_name) {
                self.code[*patch_pos] = Instr::Jump(label_pos);
            } else {
                return types::err(format!(
                    "@goto references undefined label: @label {}",
                    label_name
                ));
            }
        }
        // Clear after patching to avoid double-patching issues
        self.goto_patches.clear();
        self.label_positions.clear();
        Ok(())
    }

    pub(super) fn new_temp(&mut self, prefix: &str) -> String {
        loop {
            self.temp_counter += 1;
            let name = format!("#{prefix}#{}", self.temp_counter);
            if !self.locals.contains_key(&name) && !self.declared_globals.contains(&name) {
                return name;
            }
        }
    }

    /// Resolve an explicit `global x` against the module that owns the current
    /// compiler. Frame-0 storage is flat, so module bindings use qualified keys.
    pub(super) fn declared_global_runtime_name(&self, name: &str) -> String {
        match &self.current_module_path {
            Some(module_path) if !name.contains('.') => format!("{module_path}.{name}"),
            _ => name.to_string(),
        }
    }

    /// Emit a frame-zero read for a `global` declaration through the owning
    /// module's canonical runtime key.
    pub(super) fn emit_load_declared_global(&mut self, name: &str) {
        self.emit(Instr::LoadGlobalAny(
            self.declared_global_runtime_name(name),
        ));
    }

    /// Emit a frame-zero write for a `global` declaration through the owning
    /// module's canonical runtime key.
    pub(super) fn emit_store_declared_global(&mut self, name: &str) {
        self.emit(Instr::StoreGlobalAny(
            self.declared_global_runtime_name(name),
        ));
    }

    /// Call BEFORE a `for`/`foreach`/comprehension construct binds its own
    /// induction variable(s), to shadow (not overwrite) an already-live
    /// same-named local from an enclosing scope (Issue #10984 / #10903).
    ///
    /// Upstream Julia semantics: a `for`/comprehension induction variable is
    /// always a FRESH binding, distinct from any pre-existing same-named
    /// local — it shadows the outer binding only for the construct's
    /// lifetime; the outer binding, unchanged, is visible again once the
    /// construct's normal-completion/`break` exit is reached.
    ///
    /// If `name` has no live outer binding (the overwhelming common case —
    /// the induction variable is a fresh name, or this is the first use),
    /// this is a no-op and `shadow_local_exit` on the returned value is also
    /// a no-op: zero behavioral/perf change outside the collision case.
    ///
    /// Mirrors the physical-value-to-temp-slot save/restore idiom already
    /// used by `Expr::LetBlock` shadowing (`compile/expr/mod.rs`), which
    /// fixed the analogous #1361/#9313/#7570 let-scope bugs: `load_local`
    /// reads the outer value through its own type's Load instruction, but the
    /// temp itself is always a generic `StoreAny` slot so the restore side
    /// does not need to remember (or re-derive) the outer `ValueType`.
    ///
    /// A `name` declared `global` is left untouched — `global` rebinding
    /// already routes through `StoreGlobalAny`/frame 0 and is not a lexical
    /// shadow at all (see `store_local`/`load_local`).
    ///
    /// `initialized_locals.contains(name)` alone is NOT a safe test for "a
    /// genuine, guaranteed-live outer value exists right now" (Issue #10984
    /// follow-up, found via a sibling-loop crash in `channels.jl`'s
    /// `_wake_all_channel_waiters`): two SIBLING `for`/`foreach` constructs
    /// binding the same fresh (non-outer) name, where the first has zero
    /// runtime iterations, leave `name` marked initialized at compile time
    /// (the construct's own var-declaration bookkeeping is unconditional)
    /// even though its runtime slot was never actually stored to (the store
    /// lives inside the loop body, which never ran). If the second
    /// construct's `shadow_local_enter` mistook that residue for a genuine
    /// outer value and emitted a `load_local` to snapshot it, that load
    /// crashes with `UndefVarError` at runtime. Requiring a resolved static
    /// type (`locals.get(name)`) in addition to `initialized_locals` does
    /// not fix this by itself (the pre-existing `Stmt::For` arm inserts a
    /// type unconditionally too) — the fix is two-sided: symmetric
    /// bookkeeping restoration on exit (`restore_shadow_bookkeeping`) plus
    /// an `IsDefined` RUNTIME guard around the save/restore bytecode (see
    /// the inline comment in the body below).
    ///
    /// Known residual (documented, not fixed here — Issue #10984 follow-up):
    /// this brackets the construct's normal/`break`-exit convergence point in
    /// straight-line bytecode. An exception thrown from inside the
    /// construct's body and caught by an enclosing `try`/`catch` in the same
    /// function unwinds past this bracket without running the restore, same
    /// as the pre-existing `LetBlock` restore's own unwind gap.
    pub(super) fn shadow_local_enter(&mut self, name: &str) -> CResult<ShadowedLocal> {
        if self.declared_globals.contains(name) {
            return Ok(ShadowedLocal {
                name: name.to_string(),
                kind: ShadowedLocalKind::Global,
            });
        }

        // Snapshot pre-enter membership in ALL FIVE bookkeeping structures,
        // regardless of whether this turns out to be a genuine collision.
        // `shadow_local_exit` restores every one of them symmetrically, so
        // a construct that finds no genuine outer value leaves the maps
        // EXACTLY as it found them (i.e. `name` reverts to "not a local"),
        // instead of leaving behind a phantom "initialized" entry that a
        // later sibling construct could mistake for a real outer binding.
        let prior_bookkeeping = PriorLocalBookkeeping {
            was_initialized: self.initialized_locals.contains(name),
            ty: self.locals.get(name).cloned(),
            julia_type: self.julia_type_locals.get(name).cloned(),
            known_any_rank: self.known_any_rank_array_locals.contains(name),
            mixed_type: self.mixed_type_vars.contains(name),
        };

        let Some(outer_ty) = prior_bookkeeping.ty.clone() else {
            return Ok(ShadowedLocal {
                name: name.to_string(),
                kind: ShadowedLocalKind::NoRuntimeValue(prior_bookkeeping),
            });
        };

        // Emission gate: only a name marked initialized gets the (guarded)
        // save/restore bracket. The whole-function pre-scan seeds `locals`
        // types for EVERY local — including each loop's own fresh induction
        // variable — before any statement compiles, so gating on the type
        // entry alone would emit the bracket (and its per-loop-entry
        // `IsDefined` name lookup) for every loop in every function. The
        // shadowing loop arms themselves insert `initialized_locals` for
        // their induction variable right after calling this method
        // (truthful: the counter/element slot IS stored before each body
        // run), so a NESTED same-name construct still sees
        // `was_initialized == true` and gets the guarded save — while a
        // fresh, never-yet-compiled name stays a zero-bytecode no-op.
        if !prior_bookkeeping.was_initialized {
            return Ok(ShadowedLocal {
                name: name.to_string(),
                kind: ShadowedLocalKind::NoRuntimeValue(prior_bookkeeping),
            });
        }

        // Runtime-guarded save (Issue #10984, codex-review hardening): even
        // `was_initialized` proves only that SOME assignment was compiled
        // earlier on SOME path, not that the slot is definitely stored on
        // every runtime path — it is set by straight-line codegen even
        // inside a conditional branch. Two reachable shapes where an
        // unguarded save load crashed with UndefVarError even though
        // upstream julia runs the program fine:
        //   (a) conditionally-initialized outer local:
        //       `if flag; x = 1; end; ...; for x in 1:3 ... end` with
        //       flag == false, in a non-slotized frame (e.g. a function
        //       containing try/catch);
        //   (b) the sibling zero-iteration foreach residue described above
        //       (also independently fixed by the symmetric bookkeeping
        //       restore, which removes the residue itself).
        // So the save is bracketed by `IsDefined` at RUNTIME: a fresh Bool
        // flag slot records whether the outer value actually existed, the
        // save load runs only when it did, and `shadow_local_exit` restores
        // only under the same flag.
        let save_slot = self.new_temp(&format!("shadow_{name}"));
        let flag_slot = self.new_temp(&format!("shadowflag_{name}"));
        self.emit(Instr::PushBool(false));
        self.emit(Instr::StoreAny(flag_slot.clone()));
        self.emit(Instr::IsDefined(name.to_string()));
        let j_skip_save = self.here();
        self.emit(Instr::JumpIfZero(usize::MAX));
        self.load_local(name)?;
        self.emit(Instr::StoreAny(save_slot.clone()));
        self.emit(Instr::PushBool(true));
        self.emit(Instr::StoreAny(flag_slot.clone()));
        let after_save = self.here();
        self.patch_jump(j_skip_save, after_save);
        Ok(ShadowedLocal {
            name: name.to_string(),
            kind: ShadowedLocalKind::Collision {
                bookkeeping: prior_bookkeeping,
                outer_ty,
                save_slot,
                flag_slot,
            },
        })
    }

    /// Call AFTER a shadowing construct's body reaches its single normal-
    /// completion/`break` exit convergence point, restoring the outer
    /// binding saved by `shadow_local_enter`. See that method's doc comment
    /// for the exact contract (no-op when `name` is `global`; restores
    /// compile-time bookkeeping to its pre-enter state even when there was
    /// no genuine runtime value to save/restore; the documented
    /// exception-unwind residual).
    pub(super) fn shadow_local_exit(&mut self, shadow: ShadowedLocal) {
        match shadow.kind {
            ShadowedLocalKind::Global => {}
            ShadowedLocalKind::NoRuntimeValue(bookkeeping) => {
                self.restore_shadow_bookkeeping(&shadow.name, bookkeeping);
            }
            ShadowedLocalKind::Collision {
                bookkeeping,
                outer_ty,
                save_slot,
                flag_slot,
            } => {
                // Restore only when the runtime guard actually saved a value
                // (see the matching `IsDefined` bracket in
                // `shadow_local_enter`).
                self.emit(Instr::LoadAny(flag_slot));
                let j_skip_restore = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX));
                self.emit(Instr::LoadAny(save_slot));
                self.store_local(&shadow.name, outer_ty);
                let after_restore = self.here();
                self.patch_jump(j_skip_restore, after_restore);
                self.restore_shadow_bookkeeping(&shadow.name, bookkeeping);
            }
        }
    }

    /// Exit an explicit hard-scope local. Unlike loop shadowing, a hard-scope
    /// declaration must also forget its runtime slot when no outer value was
    /// saved. The guarded collision path restores a live outer value or forgets
    /// the clause-local slot when the outer was undefined on this runtime path.
    pub(super) fn hard_scope_shadow_exit(&mut self, shadow: ShadowedLocal) {
        match shadow.kind {
            ShadowedLocalKind::Global => {}
            ShadowedLocalKind::NoRuntimeValue(bookkeeping) => {
                self.emit(Instr::ForgetLetLocals(vec![shadow.name.clone()]));
                self.restore_shadow_bookkeeping(&shadow.name, bookkeeping);
            }
            ShadowedLocalKind::Collision {
                bookkeeping,
                outer_ty,
                save_slot,
                flag_slot,
            } => {
                self.emit(Instr::LoadAny(flag_slot.clone()));
                let j_forget = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX));
                self.emit(Instr::LoadAny(save_slot.clone()));
                self.store_local(&shadow.name, outer_ty);
                let j_end = self.here();
                self.emit(Instr::Jump(usize::MAX));
                let forget = self.here();
                self.emit(Instr::ForgetLetLocals(vec![shadow.name.clone()]));
                let end = self.here();
                self.patch_jump(j_forget, forget);
                self.patch_jump(j_end, end);
                self.emit(Instr::ForgetLetLocals(vec![save_slot, flag_slot]));
                self.restore_shadow_bookkeeping(&shadow.name, bookkeeping);
            }
        }
    }

    /// Restore `initialized_locals`/`locals`/`julia_type_locals`/
    /// `known_any_rank_array_locals`/`mixed_type_vars` membership for `name`
    /// to exactly what `shadow_local_enter` observed before the shadowing
    /// construct's own codegen ran. When there was no genuine outer value
    /// (`PriorLocalBookkeeping::ty` is `None`), this REMOVES `name` from
    /// every one of these maps rather than leaving it initialized, so a
    /// sibling shadowing construct reusing the same fresh name cannot
    /// mistake this construct's own (possibly-never-executed) binding for a
    /// live outer value (Issue #10984 follow-up).
    fn restore_shadow_bookkeeping(&mut self, name: &str, bookkeeping: PriorLocalBookkeeping) {
        if bookkeeping.was_initialized {
            self.initialized_locals.insert(name.to_string());
        } else {
            self.initialized_locals.remove(name);
        }
        match bookkeeping.ty {
            Some(ty) => {
                self.locals.insert(name.to_string(), ty);
            }
            None => {
                self.locals.remove(name);
            }
        }
        match bookkeeping.julia_type {
            Some(jt) => {
                self.julia_type_locals.insert(name.to_string(), jt);
            }
            None => {
                self.julia_type_locals.remove(name);
            }
        }
        if bookkeeping.known_any_rank {
            self.known_any_rank_array_locals.insert(name.to_string());
        } else {
            self.known_any_rank_array_locals.remove(name);
        }
        if bookkeeping.mixed_type {
            self.mixed_type_vars.insert(name.to_string());
        } else {
            self.mixed_type_vars.remove(name);
        }
    }
}

/// Saved state for one shadowed local, produced by `CoreCompiler::shadow_local_enter`
/// and consumed by `CoreCompiler::shadow_local_exit` (Issue #10984 / #10903).
#[derive(Debug, Clone)]
pub(super) struct ShadowedLocal {
    name: String,
    kind: ShadowedLocalKind,
}

#[derive(Debug, Clone)]
enum ShadowedLocalKind {
    /// `name` is declared `global` — untouched by shadowing at all (`global`
    /// rebinding routes through `StoreGlobalAny`/frame 0, not a lexical
    /// local). A true no-op on both enter and exit.
    Global,
    /// `name` had no compile-time type entry at enter time (never compiled
    /// as a local on any earlier path). No runtime save/restore bytecode is
    /// emitted; only the bookkeeping maps are restored to their pre-enter
    /// membership on exit.
    NoRuntimeValue(PriorLocalBookkeeping),
    /// `name` MAY have a live outer value (a compile-time type entry
    /// exists): its bookkeeping is snapshotted, and its runtime value is
    /// saved/restored under an `IsDefined` runtime guard — the compile-time
    /// entry alone cannot prove the slot is stored on every runtime path
    /// (conditional assignment, zero-iteration sibling loops).
    Collision {
        bookkeeping: PriorLocalBookkeeping,
        outer_ty: ValueType,
        /// Fresh temp slot holding the outer binding's runtime value across
        /// the shadow. The physical VM slot for `name` itself is reused by
        /// the shadowing construct (slots are keyed by name), so the outer
        /// value must be copied out before the construct's own binding
        /// overwrites it, and copied back on exit.
        save_slot: String,
        /// Fresh temp Bool slot: `true` iff the `IsDefined`-guarded save
        /// actually ran, so the exit restores only a genuinely-saved value.
        flag_slot: String,
    },
}

/// Pre-enter membership snapshot of the five name-keyed `CoreCompiler`
/// bookkeeping structures that track a local's compile-time state, taken by
/// `shadow_local_enter` and restored verbatim by `restore_shadow_bookkeeping`
/// on exit (Issue #10984 / #10903).
#[derive(Debug, Clone)]
struct PriorLocalBookkeeping {
    was_initialized: bool,
    ty: Option<ValueType>,
    julia_type: Option<JuliaType>,
    known_any_rank: bool,
    mixed_type: bool,
}

/// Issue #9124: element type of an `Array`/`Vector`/`Matrix`-of-struct field
/// declaration (e.g. `pts::Vector{Point}`), resolved to a `ConcreteType::Struct`
/// via `struct_table`. Returns `None` for a scalar field, an array of a
/// non-struct element type, or an unresolvable struct name — callers keep
/// the field's existing (bare) inference in those cases.
fn array_of_struct_element_concrete(
    field_jt: &JuliaType,
    struct_table: &super::context::StructRegistry,
) -> Option<crate::compile::lattice::types::ConcreteType> {
    let inner = match field_jt {
        JuliaType::VectorOf(inner) | JuliaType::MatrixOf(inner) => inner.as_ref(),
        _ => return None,
    };
    let JuliaType::Struct(name) = inner else {
        return None;
    };
    let (_, info) = struct_table.resolve(name)?;
    Some(crate::compile::lattice::types::ConcreteType::Struct {
        name: name.clone(),
        type_id: info.type_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytecode::ValueType;

    // ── is_integer_type ───────────────────────────────────────────────────────

    #[test]
    fn test_is_integer_type_signed() {
        assert!(is_integer_type(&ValueType::I64), "I64 should be integer");
        assert!(is_integer_type(&ValueType::I8), "I8 should be integer");
        assert!(is_integer_type(&ValueType::I16), "I16 should be integer");
        assert!(is_integer_type(&ValueType::I32), "I32 should be integer");
        assert!(is_integer_type(&ValueType::I128), "I128 should be integer");
    }

    #[test]
    fn test_is_integer_type_unsigned() {
        assert!(is_integer_type(&ValueType::U8), "U8 should be integer");
        assert!(is_integer_type(&ValueType::U16), "U16 should be integer");
        assert!(is_integer_type(&ValueType::U32), "U32 should be integer");
        assert!(is_integer_type(&ValueType::U64), "U64 should be integer");
        assert!(is_integer_type(&ValueType::U128), "U128 should be integer");
    }

    #[test]
    fn test_is_integer_type_non_integer() {
        assert!(!is_integer_type(&ValueType::F64), "F64 is not integer");
        assert!(!is_integer_type(&ValueType::F32), "F32 is not integer");
        assert!(!is_integer_type(&ValueType::Bool), "Bool is not integer");
        assert!(!is_integer_type(&ValueType::Str), "Str is not integer");
        assert!(!is_integer_type(&ValueType::Any), "Any is not integer");
        assert!(
            !is_integer_type(&ValueType::Nothing),
            "Nothing is not integer"
        );
    }

    // ── is_float_type ─────────────────────────────────────────────────────────

    #[test]
    fn test_is_float_type_floats() {
        assert!(is_float_type(&ValueType::F64), "F64 should be float");
        assert!(is_float_type(&ValueType::F32), "F32 should be float");
        assert!(is_float_type(&ValueType::F16), "F16 should be float");
    }

    #[test]
    fn test_is_float_type_non_float() {
        assert!(!is_float_type(&ValueType::I64), "I64 is not float");
        assert!(!is_float_type(&ValueType::Bool), "Bool is not float");
        assert!(!is_float_type(&ValueType::Str), "Str is not float");
        assert!(!is_float_type(&ValueType::Any), "Any is not float");
    }

    // ── is_numeric_type ───────────────────────────────────────────────────────

    #[test]
    fn test_is_numeric_type_integers_and_floats() {
        assert!(is_numeric_type(&ValueType::I64), "I64 is numeric");
        assert!(is_numeric_type(&ValueType::F64), "F64 is numeric");
        assert!(is_numeric_type(&ValueType::I8), "I8 is numeric");
        assert!(is_numeric_type(&ValueType::F32), "F32 is numeric");
    }

    #[test]
    fn test_is_numeric_type_non_numeric() {
        assert!(!is_numeric_type(&ValueType::Bool), "Bool is not numeric");
        assert!(!is_numeric_type(&ValueType::Str), "Str is not numeric");
        assert!(!is_numeric_type(&ValueType::Any), "Any is not numeric");
        assert!(
            !is_numeric_type(&ValueType::Nothing),
            "Nothing is not numeric"
        );
        assert!(!is_numeric_type(&ValueType::Char), "Char is not numeric");
    }

    // ── is_singleton_type ─────────────────────────────────────────────────────

    #[test]
    fn test_is_singleton_type_singletons() {
        assert!(
            is_singleton_type(&ValueType::Nothing),
            "Nothing is singleton"
        );
        assert!(is_singleton_type(&ValueType::Symbol), "Symbol is singleton");
        assert!(is_singleton_type(&ValueType::Char), "Char is singleton");
    }

    #[test]
    fn test_is_singleton_type_non_singletons() {
        assert!(!is_singleton_type(&ValueType::I64), "I64 is not singleton");
        assert!(!is_singleton_type(&ValueType::F64), "F64 is not singleton");
        assert!(!is_singleton_type(&ValueType::Str), "Str is not singleton");
        assert!(
            !is_singleton_type(&ValueType::Bool),
            "Bool is not singleton"
        );
        assert!(
            !is_singleton_type(&ValueType::DataType),
            "DataType equality is semantic, not identity-only"
        );
        assert!(!is_singleton_type(&ValueType::Any), "Any is not singleton");
    }
}
