//! Transfer function registry for type inference.
//!
//! This module provides the central registry for transfer functions (tfuncs),
//! which predict the return type of function calls during abstract interpretation.
//!
//! Transfer functions implement the type-level semantics of Julia functions,
//! allowing the inference engine to determine result types without executing code.
//!
//! # Metadata-bearing rules (Issue #3509)
//!
//! Each entry is now stored as a [`TransferRule`], carrying not only the
//! transfer function itself but also `min_arity`, `max_arity` and a `cost`.
//! This mirrors Julia's `add_tfunc(f, minarg, maxarg, tfunc, cost)` in
//! `julia/Compiler/src/tfuncs.jl` and sets up future inlining / cost-based
//! decisions.
//!
//! - `min_arity`, `max_arity`: validated before the transfer function fires.
//!   Calls with the wrong number of arguments are rejected early as a
//!   diagnostic and `Top` is returned, instead of relying on each tfunc to
//!   bail out individually.
//! - `cost`: a relative integer cost (no absolute units yet). For now we use
//!   `COST_CHEAP = 1` (primitive arithmetic / comparisons), `COST_MEDIUM = 10`
//!   (most builtins) and `COST_EXPENSIVE = 100` (allocating ops). This will
//!   eventually feed into inlining heuristics.
//!
//! Tfuncs registered via the legacy [`TransferFunctions::register`] keep the
//! previous semantics: arity is unconstrained and the cost defaults to
//! [`DEFAULT_COST`]. Migrating an entry to the metadata-bearing form is a
//! local change at the call site — see `register_arithmetic` for an example.

use crate::compile::abstract_interp::StructTypeInfo;
use crate::compile::diagnostics::{
    emit_unknown_function, DiagnosticReason, DiagnosticsCollector, TypeInferenceDiagnostic,
};
use crate::compile::lattice::types::{ConcreteType, LatticeType};
#[cfg(test)]
use crate::inference_core::{CorePrimitive, CoreType};
use crate::ir::core::Expr;
use std::collections::HashMap;

/// Default cost for tfuncs registered without explicit metadata.
///
/// Chosen to sit between `COST_CHEAP` and `COST_EXPENSIVE`, so legacy tfuncs
/// remain plausible but distinguishable from rules that opted in to the new
/// metadata API.
pub const DEFAULT_COST: u32 = 10;

/// Suggested cost tier for primitive arithmetic / comparison tfuncs.
pub const COST_CHEAP: u32 = 1;

/// Suggested cost tier for most builtins (collection accessors, conversions, ...).
pub const COST_MEDIUM: u32 = 10;

/// Suggested cost tier for allocating / iterating ops (sort, collect, ...).
pub const COST_EXPENSIVE: u32 = 100;

/// Read-only struct-identity lookup for contextual transfer functions
/// (Issue #5922).
///
/// Contextual tfuncs that resolve constructor results (e.g. `complex`, default
/// struct constructors) need to map a struct *name* to its compiled `type_id`.
/// The two inference authorities hold this information in different tables
/// (`SharedCompileContext::struct_table` on the expression-inference side and
/// the abstract-interp `StructTypeInfo` table on the engine side), so the
/// registry consumes it through this minimal trait instead of a concrete map
/// type.
pub trait StructIdLookup {
    /// `type_id` of an exact struct-table entry named `name`.
    fn struct_type_id(&self, name: &str) -> Option<usize>;
}

impl StructIdLookup for HashMap<String, StructTypeInfo> {
    fn struct_type_id(&self, name: &str) -> Option<usize> {
        self.get(name).map(|info| info.type_id)
    }
}

/// Expression-reference channel for HOF call-site lambda inference
/// (Issue #6604).
///
/// A plain [`TransferFn`] sees only argument *lattice types*, which is enough
/// for `map(Float64, xs)` (the callable is a named type-converter) but **not**
/// for `map(x -> x * 2.0, xs)`: the lattice type of an inline lambda argument is
/// just `Function`/`Closure`, with no way to recover the body's return type. To
/// move that rule onto the registry path (instead of the ad-hoc
/// `match function.as_str()` HOF arm in `compile/expr/infer/mod.rs`), the
/// transfer function needs (a) the function-argument **expression** and (b) a
/// callback that can infer the lambda's return type given input element types.
///
/// This is the HOF counterpart of [`StructInstantiation`]: a narrow seam that
/// exposes exactly the capability the rule needs, implemented on the
/// expression-inference side (`CoreCompiler`) and injected through
/// [`TFuncContext`]. As with the parametric-ctor seam, the rule body lives in
/// the expression-inference adapter, not in generic registry dispatch — see
/// the HOF adapters in `compile::expr::infer::hof`.
///
/// The analyzer is `&mut` because resolving an inline lambda's return type
/// re-enters the shared inference engine (method-table dispatch, function-IR
/// snapshots), which mutate compiler caches. That mutability is why the rule is
/// driven through a free function the adapter calls (mirroring the
/// `StructInstantiation` `&mut` seam) rather than the immutable
/// [`ContextualTransferFn`] dispatch path.
pub trait HofLambdaAnalyzer {
    /// Given the HOF's function-argument expression and the lattice type(s) of
    /// the mapped collection's element(s), return the lattice type of the
    /// mapped result element, or `None` when it cannot be determined (e.g. the
    /// callable or element type is unknown), in which case the caller falls
    /// back to the conservative registry rule.
    ///
    /// The `input_elements` slice carries one element type per mapped
    /// collection, so a unary `map(f, xs)` passes one element type while a
    /// binary `broadcast(f, xs, ys)` or n-ary `map(f, xs, ys, zs)` passes two
    /// or more (Issue #6604).
    fn map_mapped_element_type(
        &mut self,
        func_expr: &Expr,
        input_elements: &[LatticeType],
    ) -> Option<LatticeType>;

    /// Given a `reduce`/`fold` operator expression and the lattice type of the
    /// reduced collection's element, return the lattice type of the reduction
    /// result, or `None` when it cannot be determined (Issue #6604).
    ///
    /// This is distinct from [`Self::map_mapped_element_type`] because a
    /// reducer's result rule covers operators a mapped element rule does not
    /// (`^`, `&`, `|`, `xor`, user-defined `op(acc, elem)`), and the result is a
    /// scalar rather than a mapped collection element.
    fn reduce_result_type(&mut self, op_expr: &Expr, element: &LatticeType) -> Option<LatticeType>;
}

/// Context for transfer functions that need access to type information.
///
/// This context provides access to the struct table and other type information
/// that transfer functions may need to produce more precise type inference.
#[derive(Default)]
pub struct TFuncContext<'a> {
    /// Struct type information table (struct name -> StructTypeInfo)
    pub struct_table: Option<&'a HashMap<String, StructTypeInfo>>,
    /// Struct-identity lookup for constructor-style tfuncs (Issue #5922).
    pub struct_ids: Option<&'a dyn StructIdLookup>,
    /// The call's argument **expressions**, for tfuncs that must analyze the
    /// syntax of an argument (e.g. an inline HOF lambda) rather than just its
    /// lattice type (Issue #6604). Parallel to `arg_types` passed alongside.
    pub arg_exprs: Option<&'a [Expr]>,
}

impl std::fmt::Debug for TFuncContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TFuncContext")
            .field("struct_table", &self.struct_table)
            .field(
                "struct_ids",
                &self.struct_ids.map(|_| "<dyn StructIdLookup>"),
            )
            .field("arg_exprs", &self.arg_exprs.map(|e| e.len()))
            .finish()
    }
}

impl<'a> TFuncContext<'a> {
    /// Creates a new empty context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a context with a struct table.
    ///
    /// The table doubles as the [`StructIdLookup`] implementation so
    /// constructor-style contextual tfuncs work without extra wiring.
    pub fn with_struct_table(struct_table: &'a HashMap<String, StructTypeInfo>) -> Self {
        Self {
            struct_table: Some(struct_table),
            struct_ids: Some(struct_table),
            arg_exprs: None,
        }
    }

    /// Creates a context that only carries a struct-identity lookup
    /// (expression-inference side, Issue #5922).
    pub fn with_struct_ids(struct_ids: &'a dyn StructIdLookup) -> Self {
        Self {
            struct_table: None,
            struct_ids: Some(struct_ids),
            arg_exprs: None,
        }
    }

    /// Attaches the call's argument expressions to this context (Issue #6604).
    ///
    /// Used by HOF rules that need to inspect the syntax of an argument (e.g. an
    /// inline lambda body) rather than just its lattice type.
    pub fn with_arg_exprs(mut self, arg_exprs: &'a [Expr]) -> Self {
        self.arg_exprs = Some(arg_exprs);
        self
    }
}

/// Type signature for a contextual transfer function.
///
/// A contextual transfer function takes argument types and a context reference,
/// returning the inferred result type. The context provides access to type
/// information like struct definitions.
pub type ContextualTransferFn = fn(&[LatticeType], &TFuncContext) -> LatticeType;

/// Type signature for a transfer function.
///
/// A transfer function takes argument types and returns the inferred result type.
/// These functions encode type-level knowledge about Julia operations.
///
/// # Examples
/// - `+(Int64, Int64)` → `Int64`
/// - `+(Int64, Float64)` → `Float64`
/// - `length(Array{T})` → `Int64`
pub type TransferFn = fn(&[LatticeType]) -> LatticeType;

/// A metadata-bearing transfer rule (Issue #3509).
///
/// Mirrors Julia's `add_tfunc(f, minarg, maxarg, tfunc, cost)` registration
/// in `julia/Compiler/src/tfuncs.jl`. The registry validates `min_arity` and
/// `max_arity` before invoking `eval`; calls with mismatched arity return
/// [`LatticeType::Top`] and emit a diagnostic.
#[derive(Clone, Copy)]
pub struct TransferRule {
    /// Minimum number of arguments accepted by this tfunc.
    pub min_arity: usize,
    /// Maximum number of arguments accepted by this tfunc, or `None` for ∞.
    pub max_arity: Option<usize>,
    /// Relative inference / inlining cost. See `COST_*` constants.
    pub cost: u32,
    /// Whether this rule came from the legacy metadata-free registration shim.
    pub is_legacy: bool,
    /// The transfer function evaluator.
    pub eval: TransferFn,
}

impl TransferRule {
    /// Build a rule for a tfunc accepting `min_arity..=max_arity` arguments.
    pub const fn new(
        min_arity: usize,
        max_arity: Option<usize>,
        cost: u32,
        eval: TransferFn,
    ) -> Self {
        Self {
            min_arity,
            max_arity,
            cost,
            is_legacy: false,
            eval,
        }
    }

    /// Build a legacy shim rule. New production registrations should avoid
    /// this path so arity/cost metadata stays explicit.
    pub const fn legacy(eval: TransferFn) -> Self {
        Self {
            min_arity: 0,
            max_arity: None,
            cost: DEFAULT_COST,
            is_legacy: true,
            eval,
        }
    }

    /// Convenience constructor: a tfunc accepting exactly `arity` arguments.
    pub const fn exact(arity: usize, cost: u32, eval: TransferFn) -> Self {
        Self::new(arity, Some(arity), cost, eval)
    }

    /// Convenience constructor: a tfunc accepting at least `min_arity` (no upper bound).
    pub const fn at_least(min_arity: usize, cost: u32, eval: TransferFn) -> Self {
        Self::new(min_arity, None, cost, eval)
    }

    /// Returns true if `argc` satisfies the rule's arity range.
    pub fn accepts_arity(&self, argc: usize) -> bool {
        if argc < self.min_arity {
            return false;
        }
        match self.max_arity {
            Some(max) => argc <= max,
            None => true,
        }
    }
}

impl std::fmt::Debug for TransferRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransferRule")
            .field("min_arity", &self.min_arity)
            .field("max_arity", &self.max_arity)
            .field("cost", &self.cost)
            .field("is_legacy", &self.is_legacy)
            .field("eval", &"<fn>")
            .finish()
    }
}

/// Emit an arity-mismatch diagnostic and return `Top`.
///
/// Pulled into a helper so the arity check at the registry layer mirrors the
/// existing `emit_unknown_function` style.
fn emit_arity_mismatch(name: &str, rule: &TransferRule, actual: usize) -> LatticeType {
    let max_repr = match rule.max_arity {
        Some(m) => m.to_string(),
        None => "∞".to_string(),
    };
    let context = format!(
        "call to {} with {} arg(s); expected {}..{}",
        name, actual, rule.min_arity, max_repr,
    );
    DiagnosticsCollector::emit(
        TypeInferenceDiagnostic::new(DiagnosticReason::Other(format!(
            "arity mismatch for tfunc '{}': got {}, expected {}..{}",
            name, actual, rule.min_arity, max_repr,
        )))
        .with_context(context),
    );
    LatticeType::Top
}

/// Registry of transfer functions for type inference.
///
/// The registry maps function names to metadata-bearing [`TransferRule`]s,
/// which predict return types based on argument types while also carrying
/// arity bounds and cost information for future optimizer use.
///
/// # Design
/// - Functions are registered by name (e.g., "+", "length", "getindex")
/// - Each function has a single transfer function rule covering its arity range
/// - Transfer functions use pattern matching on argument types
/// - Unknown functions return `Top` (Any) and emit a diagnostic
/// - Arity mismatches return `Top` and emit a diagnostic
///
/// # Example
/// ```
/// use subset_julia_vm::compile::tfuncs::TransferFunctions;
/// use subset_julia_vm::compile::lattice::types::{ConcreteType, LatticeType};
///
/// let mut registry = TransferFunctions::new();
/// // In actual usage, you would register transfer functions here
/// // registry.register("+", arithmetic::tfunc_add);
///
/// let args = vec![
///     LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))),
///     LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))),
/// ];
/// // For demonstration, this returns Top since no functions are registered
/// let result = registry.infer_return_type("+", &args);
/// assert_eq!(result, LatticeType::Top);
/// ```
#[derive(Debug)]
pub struct TransferFunctions {
    /// Map from function name to metadata-bearing rule.
    rules: HashMap<String, TransferRule>,
    /// Map from function name to contextual transfer function.
    /// These are used when context (like struct tables) is available.
    contextual_functions: HashMap<String, ContextualTransferFn>,
}

impl TransferFunctions {
    /// Creates a new, empty transfer function registry.
    pub fn new() -> Self {
        Self {
            rules: HashMap::new(),
            contextual_functions: HashMap::new(),
        }
    }

    /// Registers a transfer function with default metadata (legacy shim).
    ///
    /// This preserves the behavior of the pre-#3509 registry: any arity is
    /// accepted and the cost defaults to [`DEFAULT_COST`]. New tfuncs should
    /// prefer [`TransferFunctions::register_rule`] so the optimizer can use
    /// the metadata.
    ///
    /// # Arguments
    /// - `name`: The function name (e.g., "+", "length", "getindex")
    /// - `tfunc`: The transfer function to register
    ///
    /// # Example
    /// ```
    /// use subset_julia_vm::compile::tfuncs::TransferFunctions;
    /// use subset_julia_vm::compile::lattice::types::LatticeType;
    ///
    /// let mut registry = TransferFunctions::new();
    /// registry.register("+", |_args| {
    ///     // Transfer function implementation
    ///     LatticeType::Top
    /// });
    /// ```
    pub fn register(&mut self, name: &str, tfunc: TransferFn) {
        self.rules
            .insert(name.to_string(), TransferRule::legacy(tfunc));
    }

    /// Registers a metadata-bearing rule under `name`.
    ///
    /// Mirrors Julia's `add_tfunc(f, minarg, maxarg, tfunc, cost)`.
    pub fn register_rule(&mut self, name: &str, rule: TransferRule) {
        self.rules.insert(name.to_string(), rule);
    }

    /// Convenience: register a tfunc accepting exactly `arity` arguments.
    pub fn register_exact(&mut self, name: &str, arity: usize, cost: u32, tfunc: TransferFn) {
        self.register_rule(name, TransferRule::exact(arity, cost, tfunc));
    }

    /// Convenience: register a tfunc accepting `min..=max` arguments.
    pub fn register_ranged(
        &mut self,
        name: &str,
        min_arity: usize,
        max_arity: Option<usize>,
        cost: u32,
        tfunc: TransferFn,
    ) {
        self.register_rule(name, TransferRule::new(min_arity, max_arity, cost, tfunc));
    }

    /// Look up the rule registered under `name`, if any.
    pub fn rule(&self, name: &str) -> Option<&TransferRule> {
        self.rules.get(name)
    }

    /// Cost metadata for `name`, if a rule is registered.
    pub fn cost(&self, name: &str) -> Option<u32> {
        self.rules.get(name).map(|r| r.cost)
    }

    /// Arity bounds for `name`, if a rule is registered.
    pub fn arity_bounds(&self, name: &str) -> Option<(usize, Option<usize>)> {
        self.rules.get(name).map(|r| (r.min_arity, r.max_arity))
    }

    /// Names still registered through the legacy metadata-free shim.
    pub fn legacy_rule_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .rules
            .iter()
            .filter(|&(_name, rule)| rule.is_legacy)
            .map(|(name, _rule)| name.clone())
            .collect();
        names.sort();
        names
    }

    /// Infers the return type of a function call.
    ///
    /// # Arguments
    /// - `function_name`: The name of the function being called
    /// - `arg_types`: The types of the arguments
    ///
    /// # Returns
    /// The inferred return type, or `Top` if the function is unknown,
    /// the type cannot be determined, or the arity does not match the
    /// registered rule.
    ///
    /// # Example
    /// ```
    /// use subset_julia_vm::compile::tfuncs::TransferFunctions;
    /// use subset_julia_vm::compile::lattice::types::{ConcreteType, LatticeType};
    ///
    /// let registry = TransferFunctions::new();
    /// let args = vec![
    ///     LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))),
    ///     LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64))),
    /// ];
    /// let result = registry.infer_return_type("+", &args);
    /// assert_eq!(result, LatticeType::Top); // Returns Top for unknown function
    /// ```
    pub fn infer_return_type(&self, function_name: &str, arg_types: &[LatticeType]) -> LatticeType {
        if let Some(rule) = self.rules.get(function_name) {
            if !rule.accepts_arity(arg_types.len()) {
                return emit_arity_mismatch(function_name, rule, arg_types.len());
            }
            // Transfer-function rules match on the widened type shapes
            // (`Concrete(Struct)`, primitives, ...). A `PartialStruct`
            // argument is behaviorally its struct type here — the per-field
            // facts are consumed earlier by the engine's getfield handling —
            // so widen before rule evaluation to keep every rule's
            // struct-shape match working unchanged (Issue #8544).
            if arg_types.iter().any(LatticeType::is_partial_struct) {
                let widened: Vec<LatticeType> = arg_types
                    .iter()
                    .map(LatticeType::widen_partial_struct)
                    .collect();
                return (rule.eval)(&widened);
            }
            (rule.eval)(arg_types)
        } else {
            // Unknown function: conservatively return Top (Any)
            // Emit diagnostic if enabled
            emit_unknown_function(function_name);
            LatticeType::Top
        }
    }

    /// Registers a contextual transfer function for a given function name.
    ///
    /// Contextual transfer functions have access to type information like
    /// struct definitions, enabling more precise type inference.
    ///
    /// # Arguments
    /// - `name`: The function name (e.g., "getfield")
    /// - `tfunc`: The contextual transfer function to register
    pub fn register_contextual(&mut self, name: &str, tfunc: ContextualTransferFn) {
        self.contextual_functions.insert(name.to_string(), tfunc);
    }

    /// Infers the return type of a function call with context.
    ///
    /// This method first checks for a contextual transfer function, which can
    /// use the provided context (struct table, etc.) for more precise inference.
    /// Falls back to the regular transfer function if no contextual one exists.
    ///
    /// # Arguments
    /// - `function_name`: The name of the function being called
    /// - `arg_types`: The types of the arguments
    /// - `ctx`: The context containing type information
    ///
    /// # Returns
    /// The inferred return type, or `Top` if the function is unknown.
    pub fn infer_return_type_with_context(
        &self,
        function_name: &str,
        arg_types: &[LatticeType],
        ctx: &TFuncContext,
    ) -> LatticeType {
        // First, try contextual transfer function
        if let Some(tfunc) = self.contextual_functions.get(function_name) {
            // Contextual rules also match widened struct shapes; see the
            // PartialStruct widening note in `infer_return_type` (Issue #8544).
            if arg_types.iter().any(LatticeType::is_partial_struct) {
                let widened: Vec<LatticeType> = arg_types
                    .iter()
                    .map(LatticeType::widen_partial_struct)
                    .collect();
                return tfunc(&widened, ctx);
            }
            return tfunc(arg_types, ctx);
        }

        // Fall back to regular transfer function.
        //
        // Note (Issue #5922): `struct_constructor_result` is deliberately NOT
        // applied here as a dispatch-level fallback. The abstract-interp
        // engine consults this method as its last resort for `ModuleCall` /
        // unresolved calls and depends on `Top` for pure-Julia structs whose
        // runtime representation is a builtin `ValueType` (e.g.
        // `Base.Generator` is a struct-table entry but is represented as
        // `ValueType::Generator`; resolving it to `Struct(id)` breaks codegen
        // coercion — see fixture `generator_runtime_callable_constructor`).
        self.infer_return_type(function_name, arg_types)
    }

    /// Type-level rule for a default struct constructor call (Issue #5922).
    ///
    /// Single authority for "a call whose name is an exact struct-table entry
    /// constructs that struct", consumed by the expression-inference adapters
    /// in `compile::expr::infer::expr_tfuncs` (which replaced the legacy
    /// inline `struct_table` gate in `infer_expr_type`). Callers decide
    /// *where* this rule applies; see `infer_return_type_with_context` for
    /// why it must not run during generic registry dispatch.
    pub fn struct_constructor_result(
        function_name: &str,
        ctx: &TFuncContext,
    ) -> Option<LatticeType> {
        let ids = ctx.struct_ids?;
        let type_id = ids.struct_type_id(function_name)?;
        Some(LatticeType::Concrete(ConcreteType::Struct {
            name: function_name.to_string(),
            type_id,
        }))
    }

    /// Returns true if a contextual transfer function is registered for the given function name.
    pub fn has_contextual_function(&self, function_name: &str) -> bool {
        self.contextual_functions.contains_key(function_name)
    }

    /// Returns true if a transfer function is registered for the given function name.
    pub fn has_function(&self, function_name: &str) -> bool {
        self.rules.contains_key(function_name)
    }

    /// Returns the number of registered transfer functions.
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Returns true if no transfer functions are registered.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

impl Default for TransferFunctions {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::compile::diagnostics::DiagnosticsCollector;
    use crate::compile::lattice::types::ConcreteType;

    fn dummy_tfunc(_args: &[LatticeType]) -> LatticeType {
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )))
    }

    fn binary_int_tfunc(args: &[LatticeType]) -> LatticeType {
        // Trivial illustration: same as dummy_tfunc but only valid for arity 2
        let _ = args;
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )))
    }

    #[test]
    fn test_new_registry_is_empty() {
        let registry = TransferFunctions::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_register_function() {
        let mut registry = TransferFunctions::new();
        registry.register("test_fn", dummy_tfunc);

        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);
        assert!(registry.has_function("test_fn"));
    }

    #[test]
    fn test_infer_return_type_registered() {
        let mut registry = TransferFunctions::new();
        registry.register("test_fn", dummy_tfunc);

        let result = registry.infer_return_type("test_fn", &[]);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_infer_return_type_unknown() {
        let registry = TransferFunctions::new();
        let result = registry.infer_return_type("unknown_fn", &[]);
        assert_eq!(result, LatticeType::Top);
    }

    #[test]
    fn test_has_function() {
        let mut registry = TransferFunctions::new();
        registry.register("exists", dummy_tfunc);

        assert!(registry.has_function("exists"));
        assert!(!registry.has_function("does_not_exist"));
    }

    #[test]
    fn test_multiple_registrations() {
        let mut registry = TransferFunctions::new();
        registry.register("fn1", dummy_tfunc);
        registry.register("fn2", dummy_tfunc);
        registry.register("fn3", dummy_tfunc);

        assert_eq!(registry.len(), 3);
        assert!(registry.has_function("fn1"));
        assert!(registry.has_function("fn2"));
        assert!(registry.has_function("fn3"));
    }

    #[test]
    fn test_register_rule_metadata_round_trips() {
        let mut registry = TransferFunctions::new();
        registry.register_rule(
            "binop",
            TransferRule::exact(2, COST_CHEAP, binary_int_tfunc),
        );

        let rule = registry.rule("binop").expect("rule should be present");
        assert_eq!(rule.min_arity, 2);
        assert_eq!(rule.max_arity, Some(2));
        assert_eq!(rule.cost, COST_CHEAP);

        assert_eq!(registry.cost("binop"), Some(COST_CHEAP));
        assert_eq!(registry.arity_bounds("binop"), Some((2, Some(2))));
    }

    #[test]
    fn test_default_cost_for_legacy_register() {
        let mut registry = TransferFunctions::new();
        registry.register("legacy", dummy_tfunc);

        assert_eq!(registry.cost("legacy"), Some(DEFAULT_COST));
        // Legacy shim accepts any arity (min=0, max=None).
        assert_eq!(registry.arity_bounds("legacy"), Some((0, None)));
        assert_eq!(registry.legacy_rule_names(), vec!["legacy".to_string()]);
    }

    #[test]
    fn test_arity_mismatch_returns_top_with_diagnostic() {
        DiagnosticsCollector::clear();
        DiagnosticsCollector::enable();

        let mut registry = TransferFunctions::new();
        registry.register_exact("binop", 2, COST_CHEAP, binary_int_tfunc);

        // Too few args
        let result = registry.infer_return_type("binop", &[LatticeType::Top]);
        assert_eq!(result, LatticeType::Top);

        // Too many args
        let result = registry.infer_return_type(
            "binop",
            &[LatticeType::Top, LatticeType::Top, LatticeType::Top],
        );
        assert_eq!(result, LatticeType::Top);

        // Correct arity should still dispatch normally
        let result = registry.infer_return_type("binop", &[LatticeType::Top, LatticeType::Top]);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );

        let diags = DiagnosticsCollector::take();
        assert!(
            diags.len() >= 2,
            "expected at least 2 arity-mismatch diagnostics, got {:?}",
            diags
        );
        assert!(diags.iter().all(|d| match &d.reason {
            DiagnosticReason::Other(msg) => msg.contains("arity mismatch"),
            _ => false,
        }));

        DiagnosticsCollector::disable();
    }

    #[test]
    fn test_register_ranged_accepts_inclusive_bounds() {
        let mut registry = TransferFunctions::new();
        registry.register_ranged("rng", 1, Some(3), COST_MEDIUM, dummy_tfunc);

        // 1 arg
        assert_eq!(
            registry.infer_return_type("rng", &[LatticeType::Top]),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
        // 3 args (upper bound, inclusive)
        assert_eq!(
            registry.infer_return_type(
                "rng",
                &[LatticeType::Top, LatticeType::Top, LatticeType::Top]
            ),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );

        // 0 args -> arity mismatch
        DiagnosticsCollector::clear();
        DiagnosticsCollector::enable();
        assert_eq!(registry.infer_return_type("rng", &[]), LatticeType::Top);
        let diags = DiagnosticsCollector::take();
        assert_eq!(diags.len(), 1);
        DiagnosticsCollector::disable();
    }

    struct StubStructIds;

    impl StructIdLookup for StubStructIds {
        fn struct_type_id(&self, name: &str) -> Option<usize> {
            match name {
                "Point" => Some(7),
                "Complex{Float64}" => Some(11),
                _ => None,
            }
        }
    }

    #[test]
    fn test_struct_constructor_result_resolves_exact_struct_table_entry() {
        let ids = StubStructIds;
        let ctx = TFuncContext::with_struct_ids(&ids);

        assert_eq!(
            TransferFunctions::struct_constructor_result("Point", &ctx),
            Some(LatticeType::Concrete(ConcreteType::Struct {
                name: "Point".to_string(),
                type_id: 7,
            }))
        );
        assert_eq!(
            TransferFunctions::struct_constructor_result("Missing", &ctx),
            None
        );
    }

    #[test]
    fn test_struct_constructor_result_requires_struct_ids() {
        let ctx = TFuncContext::new();
        assert_eq!(
            TransferFunctions::struct_constructor_result("Point", &ctx),
            None
        );
    }

    /// Generic registry dispatch must stay conservative (`Top`) for unknown
    /// calls whose name happens to be a struct-table entry: the engine relies
    /// on `Top` for pure-Julia structs with builtin runtime representations
    /// such as `Base.Generator` (Issue #5922, fixture
    /// `generator_runtime_callable_constructor`).
    #[test]
    fn test_dispatch_does_not_apply_struct_constructor_fallback() {
        let registry = TransferFunctions::new();
        let ids = StubStructIds;
        let ctx = TFuncContext::with_struct_ids(&ids);

        let result = registry.infer_return_type_with_context("Point", &[LatticeType::Top], &ctx);
        assert_eq!(result, LatticeType::Top);
    }

    #[test]
    fn test_struct_table_doubles_as_struct_id_lookup() {
        let mut table: HashMap<String, StructTypeInfo> = HashMap::new();
        table.insert(
            "Point".to_string(),
            StructTypeInfo::new(3, false, HashMap::new(), false),
        );
        table.insert(
            "Wrapper{Int64}".to_string(),
            StructTypeInfo::new(4, false, HashMap::new(), false),
        );
        table.insert(
            "Wrapper{Bool}".to_string(),
            StructTypeInfo::new(5, false, HashMap::new(), false),
        );

        let ctx = TFuncContext::with_struct_table(&table);
        let ids = ctx.struct_ids.expect("struct table should act as lookup");
        assert_eq!(ids.struct_type_id("Point"), Some(3));
        assert_eq!(ids.struct_type_id("Missing"), None);
        assert_eq!(ids.struct_type_id("Wrapper{Int64}"), Some(4));
        assert_eq!(ids.struct_type_id("Wrapper{Bool}"), Some(5));
        assert_eq!(ids.struct_type_id("Wrapper"), None);

        // The constructor rule resolves through the same context, while
        // generic dispatch stays conservative (see
        // `test_dispatch_does_not_apply_struct_constructor_fallback`).
        assert_eq!(
            TransferFunctions::struct_constructor_result("Point", &ctx),
            Some(LatticeType::Concrete(ConcreteType::Struct {
                name: "Point".to_string(),
                type_id: 3,
            }))
        );
        let registry = TransferFunctions::new();
        let result = registry.infer_return_type_with_context("Point", &[], &ctx);
        assert_eq!(result, LatticeType::Top);
    }

    #[test]
    fn test_transfer_rule_accepts_arity_bounds() {
        let exact = TransferRule::exact(2, COST_CHEAP, dummy_tfunc);
        assert!(!exact.accepts_arity(1));
        assert!(exact.accepts_arity(2));
        assert!(!exact.accepts_arity(3));

        let unbounded = TransferRule::at_least(1, COST_MEDIUM, dummy_tfunc);
        assert!(!unbounded.accepts_arity(0));
        assert!(unbounded.accepts_arity(1));
        assert!(unbounded.accepts_arity(100));
    }
}
