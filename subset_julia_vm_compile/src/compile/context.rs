//! Shared compilation context for parametric type instantiation.
//!
//! This module manages struct definitions, parametric type instantiation,
//! and type information that is shared across all compiler instances.

use std::collections::{HashMap, HashSet};

use crate::bytecode::{AbstractTypeDefInfo, PrimitiveTypeDefInfo, StructDefInfo, ValueType};
use crate::ir::core::{Block, Function, MacroDef};
use crate::types::{parse_single_type_expr, JuliaType, TypeExpr, TypeParam};
use subset_julia_vm_types::runtime_types::parametric;

use super::method_table::MethodSig;
use super::types::{
    err, parse_parametric_call, parse_type_args_recursive, CResult, CompileError, InstantiationKey,
    ParametricStructDef,
};
use super::{check_type_satisfies_bound, julia_type_to_value_type};

/// Macro definition info for compilation.
/// Stored in SharedCompileContext for macro expansion during lowering/compilation.
/// Macro expansion support is staged; some fields are currently reserved.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct MacroInfo {
    /// Parameter names (macro receives AST nodes, not values)
    pub params: Vec<String>,
    /// The macro body block
    pub body: Block,
}

impl From<&MacroDef> for MacroInfo {
    fn from(def: &MacroDef) -> Self {
        Self {
            params: def.params.clone(),
            body: def.body.clone(),
        }
    }
}

/// A method row in definition order, with optional source-point visibility.
#[derive(Debug, Clone)]
pub(crate) struct SourceOrderedMethodSig {
    pub(crate) sig: MethodSig,
    /// `None` means visible for the whole compilation unit (Base/cache/module
    /// rows). `Some(start)` means a top-level call only sees it at spans starting
    /// at or after that user method definition.
    pub(crate) visible_from_source_start: Option<usize>,
}

/// `@enum` definition info for compilation (Issue #5139).
///
/// Collected in a pre-pass so that bare references to the enum type name and to
/// its members resolve regardless of statement order, and so that
/// `Color(value)` / `instances(Color)` can be recognized at their call sites.
#[derive(Debug, Clone)]
pub struct EnumInfo {
    /// Underlying integer type (e.g. "Int32").
    pub base_type: String,
    /// `(member_name, value)` pairs in declaration order.
    pub members: Vec<(String, i64)>,
}

/// Re-export shim: `StructInfo` is owned by the shared runtime type layer
/// (`crate::runtime_types::struct_info`) since Issue #8557; the
/// `compile::context::StructInfo` / `compile::StructInfo` paths stay valid
/// for existing users.
pub use crate::runtime_types::struct_info::StructInfo;

/// Owner-scoped struct identity and the table keyed by it (Issue #11078,
/// Phase 2b of #10459). `StructRegistry` replaces the `HashMap<String,
/// StructInfo>` this module used to carry: entries are keyed by `StructId`,
/// and names are aliases into that id space.
pub use subset_julia_vm_bytecode::{StructId, StructRegistry};

/// Position of a type declaration within the current lowered source.
///
/// Most top-level definitions carry a monotonic `definition_order`. Lifted
/// methods inside hard scopes can still carry zero, so signature-probe ordering
/// falls back to the byte offset when either side lacks an ordinal (#11117).
#[derive(Debug, Clone, Copy)]
pub struct TypeDefinitionPosition {
    pub definition_order: u64,
    pub source_start: usize,
}

impl TypeDefinitionPosition {
    pub fn is_before(self, other_definition_order: u64, other_source_start: usize) -> bool {
        if crate::ir::core::is_source_definition_order(self.definition_order)
            && crate::ir::core::is_source_definition_order(other_definition_order)
        {
            self.definition_order < other_definition_order
        } else {
            self.source_start < other_source_start
        }
    }
}

/// Shared compilation context for parametric type instantiation.
/// This is shared across all compiler instances to track type instantiations.
pub struct SharedCompileContext {
    pub struct_table: StructRegistry,
    pub struct_defs: Vec<StructDefInfo>,
    pub struct_name_to_def_index: HashMap<String, usize>,
    pub parametric_structs: HashMap<String, ParametricStructDef>,
    pub base_parametric_structs: HashMap<String, ParametricStructDef>,
    pub abstract_types: Vec<AbstractTypeDefInfo>,
    pub abstract_type_by_name: HashMap<String, usize>,
    pub type_id_to_struct_name: HashMap<usize, String>,
    pub instantiation_table: HashMap<InstantiationKey, usize>,
    pub next_type_id: usize,
    /// Source position of each type defined by the current input.
    ///
    /// The struct table is populated for the WHOLE program regardless of source
    /// order, so "the compiler can resolve this name as a type" says nothing
    /// about whether the type EXISTS YET at a given definition site. Upstream
    /// Julia evaluates a signature's annotations eagerly when the method
    /// definition executes, so a FORWARD reference (`f(x::S) = 1` before
    /// `struct S end`) raises `UndefVarError` there. Inherited Base/cache types
    /// are deliberately absent because their source positions are unrelated to
    /// this input. The definition-time signature probes compare positions only
    /// for entries in this map (Issues #11025/#11117).
    pub type_definition_positions: HashMap<String, TypeDefinitionPosition>,
    /// Module-body value bindings and where their first plain assignment sits
    /// relative to the import chronology. Maps the qualified binding name
    /// (`"Sink.A"`) to the `definition_order` of the last `Stmt::Using` marker
    /// preceding the first assignment (0 when the assignment precedes every
    /// import). An import binding the same name whose own `definition_order`
    /// is greater hits upstream's warn-and-ignore conflict (Issue #11426).
    pub module_value_binding_positions: HashMap<String, u64>,
    /// Top-level (global/const) variable types, available to all functions.
    pub global_types: HashMap<String, ValueType>,
    /// Top-level binding types for runtime reflection. This mirrors
    /// `global_types` after widening non-const globals to `Any`; user bindings
    /// remain visible because reflection observes the executed program state.
    pub inference_global_types: HashMap<String, ValueType>,
    /// Const struct constructor calls that can be inlined.
    /// Maps variable name -> (struct_name, type_id, field_count)
    /// For `const M = MyType()`, stores ("M" -> ("MyType", type_id, 0))
    pub global_const_structs: HashMap<String, (String, usize, usize)>,
    /// Lazy AoT: Maps function global_index -> specializable_functions index
    /// Used by expression compiler to emit CallSpecialize instead of Call
    pub spec_func_mapping: HashMap<usize, usize>,
    /// Macro definitions for compile-time expansion
    /// Stored ahead of full macro compilation support to keep context shape stable.
    #[allow(dead_code)]
    pub macros: HashMap<String, MacroInfo>,
    /// Map from function name to its index in function_infos.
    /// Used to look up functions defined inside blocks (Stmt::FunctionDef).
    pub function_indices: HashMap<String, usize>,
    /// Exact source definition span start -> function_infos index.
    ///
    /// A name can be redefined multiple times in one script. `function_indices`
    /// intentionally points to the latest binding, so top-level activation needs
    /// this span-keyed map to activate the definition statement that is actually
    /// executing.
    /// Every compiled body generated by one source definition, keyed by that
    /// definition's start span. Optional positional arguments can generate a
    /// primary body plus one or more wrappers at the same source position.
    pub function_indices_by_span_start: HashMap<usize, Vec<usize>>,
    /// Function names that may gain methods through runtime `@eval`.
    pub runtime_eval_function_names: HashSet<String>,
    /// Global indices of methods introduced by runtime `@eval`.
    pub runtime_eval_function_indices: HashSet<usize>,
    /// Function names that a statically-visible runtime `eval(:(f(...) = ...))`
    /// can define or redefine.
    pub opaque_runtime_eval_function_names: HashSet<String>,
    /// Nominal bindings introduced by runtime-conditional statements already
    /// encountered in source order. Calls use the runtime callable-value path
    /// because these names are not active compile-time type identities.
    pub runtime_nominal_callable_names: HashSet<String>,
    /// Runtime-conditional nominal declarations originating in the raw current
    /// input, before Base/prelude/package merging and optimization.
    pub current_input_runtime_nominal_names: HashSet<String>,
    /// Runtime declaration site -> dormant inner-constructor function rows.
    /// The declaration's branch-gated marker emits their activation markers
    /// only after the reserved type has been published (Issue #11679).
    pub runtime_nominal_constructor_indices: HashMap<u64, Vec<usize>>,
    /// True when the user program contains an opaque runtime code-evaluation
    /// call (`eval`, `include_string`, or `evalfile`) whose target definitions
    /// cannot be named statically.
    pub has_opaque_runtime_eval: bool,
    /// Map from global function index to its IR for call-site type inference.
    pub function_ir_by_global_index: HashMap<usize, Function>,
    /// Method rows in registration order, used to reconstruct a source-visible
    /// method table for top-level script calls before a later redefinition.
    pub(crate) source_ordered_method_sigs: HashMap<String, Vec<SourceOrderedMethodSig>>,
    /// Root-script top-level function bodies that may need source-world runtime
    /// filtering when they call a same-signature method redefined later in the
    /// script (Issue #9650).
    pub(crate) source_world_function_names: HashSet<String>,
    /// The current compilation is a REPL delta whose newly appended method
    /// bodies remain dormant until source-order activation markers execute.
    pub(crate) repl_source_ordered_dispatch: bool,
    /// Type aliases: maps alias name -> target type name
    /// For `const MyInt = Int64`, stores ("MyInt" -> "Int64")
    pub type_aliases: HashMap<String, String>,
    /// Re-exported (imported) bindings via selective `import/using Src: a, b`.
    /// Maps the importing module's qualified name (e.g. `"Facade.T"`) to the
    /// resolved source qualified name (e.g. `"Defn.T"`) so module-qualified
    /// access to a re-exported binding (`Facade.T`, `Facade.g(t)`) resolves to
    /// its source definition, matching Julia's `getproperty`-via-imports
    /// behavior (Issue #8053).
    pub module_imported_bindings: HashMap<String, String>,
    /// Public (but not necessarily exported) names keyed by qualified module
    /// path. Used when a module value must be synthesized before the
    /// authoritative runtime binding is initialized.
    pub module_publics: HashMap<String, Vec<String>>,
    /// Closure captured variables: maps function name -> set of captured variable names.
    /// Used when compiling closures to know which variables to load via LoadCaptured.
    pub closure_captures: HashMap<String, std::collections::HashSet<String>>,
    /// Names defined by `global function f(...) ... end` inside a module-level
    /// `let` scope whose body captures that scope's locals (the Base bootstrap
    /// pattern, Issue #11015). The definition binds a CLOSURE value to the
    /// module-level name, so every call site must route through that value
    /// instead of direct-dispatching to the method — the method body loads its
    /// captures with `LoadCaptured` and only the closure carries them.
    pub let_scope_global_closures: HashSet<String>,
    /// `@enum` types by name, populated in a pre-pass (Issue #5139). Used to
    /// resolve bare type-name / member references and `Color(v)` / `instances`.
    pub enum_types: HashMap<String, EnumInfo>,
    /// User-declared primitive types by name (`primitive type Name Bits end`,
    /// Issue #5058). A bare reference to one of these names resolves to its
    /// `DataType` value (so `MyBits isa Type`, `===`, `<:` work) and the runtime
    /// type-reflection layer answers `isprimitivetype`/`sizeof`/`supertype`.
    pub primitive_types: Vec<PrimitiveTypeDefInfo>,
    /// Name -> index lookup into `primitive_types`.
    pub primitive_type_by_name: HashMap<String, usize>,
    /// Parametric-struct base name (e.g. `"Rational{"` for `struct
    /// Rational{T} ... end`) -> the `type_id` of a matching concrete
    /// instantiation currently in `struct_table` (Issue #10129).
    ///
    /// `resolve_global_types()` falls back to "any instantiation of this
    /// parametric struct family" when an exact `struct_table` lookup misses
    /// (typically a REPL-carried global whose recorded struct name is a
    /// generic/uninstantiated spelling). That used to be a linear scan of the
    /// whole `struct_table` per lookup; this index makes it O(1). Kept in
    /// sync with `struct_table`: seeded once from its initial contents in
    /// [`Self::with_instantiation_table`] and updated at the single runtime
    /// insertion point, [`Self::resolve_instantiation_with_type_expr`]. A
    /// later instantiation overwrites an earlier one sharing the same prefix,
    /// which mirrors "prefer the current struct_table state" — the two only
    /// disagree when multiple concrete instantiations of the same parametric
    /// family coexist, a case the original linear scan resolved via
    /// unspecified `HashMap` iteration order anyway.
    pub(crate) parametric_struct_prefix_index: HashMap<String, usize>,
}

impl SharedCompileContext {
    pub fn new(
        struct_table: StructRegistry,
        struct_defs: Vec<StructDefInfo>,
        parametric_structs: HashMap<String, ParametricStructDef>,
        base_parametric_structs: HashMap<String, ParametricStructDef>,
        abstract_types: Vec<AbstractTypeDefInfo>,
        next_type_id: usize,
    ) -> Self {
        Self::with_instantiation_table(
            struct_table,
            struct_defs,
            parametric_structs,
            base_parametric_structs,
            abstract_types,
            next_type_id,
            HashMap::new(),
        )
    }

    /// Create with a pre-populated instantiation table (for caching).
    #[allow(clippy::too_many_arguments)]
    pub fn with_instantiation_table(
        struct_table: StructRegistry,
        struct_defs: Vec<StructDefInfo>,
        parametric_structs: HashMap<String, ParametricStructDef>,
        base_parametric_structs: HashMap<String, ParametricStructDef>,
        abstract_types: Vec<AbstractTypeDefInfo>,
        next_type_id: usize,
        instantiation_table: HashMap<InstantiationKey, usize>,
    ) -> Self {
        let mut struct_name_to_def_index = HashMap::new();
        for (idx, def) in struct_defs.iter().enumerate() {
            struct_name_to_def_index.insert(def.name.clone(), idx);
        }

        // Issue #11046: owner-exact lookup recovers a shadowed Main/Base
        // declaration directly from the registry. No parallel alias table is
        // needed now that declaration identity survives lexical collisions.
        let mut type_id_to_struct_name = HashMap::new();
        for (idx, def) in struct_defs.iter().enumerate() {
            let type_id = struct_table
                .resolve_scoped(&def.name, None, true)
                .map(|(_, info)| info.type_id)
                .unwrap_or(idx);
            type_id_to_struct_name
                .entry(type_id)
                .or_insert_with(|| def.name.clone());
        }
        let mut parametric_struct_prefix_index: HashMap<String, usize> = HashMap::new();
        for (name, info) in &struct_table {
            type_id_to_struct_name
                .entry(info.type_id)
                .or_insert_with(|| name.clone());
            if let Some(brace_idx) = name.find('{') {
                parametric_struct_prefix_index.insert(name[..=brace_idx].to_string(), info.type_id);
            }
        }

        let mut abstract_type_by_name = HashMap::new();
        for (idx, at) in abstract_types.iter().enumerate() {
            abstract_type_by_name.insert(at.name.clone(), idx);
        }

        Self {
            struct_table,
            struct_defs,
            struct_name_to_def_index,
            parametric_structs,
            base_parametric_structs,
            abstract_types,
            abstract_type_by_name,
            type_id_to_struct_name,
            parametric_struct_prefix_index,
            instantiation_table,
            next_type_id,
            type_definition_positions: HashMap::new(),
            module_value_binding_positions: HashMap::new(),
            global_types: HashMap::new(),
            inference_global_types: HashMap::new(),
            global_const_structs: HashMap::new(),
            spec_func_mapping: HashMap::new(),
            macros: HashMap::new(),
            function_indices: HashMap::new(),
            function_indices_by_span_start: HashMap::new(),
            runtime_eval_function_names: HashSet::new(),
            runtime_eval_function_indices: HashSet::new(),
            opaque_runtime_eval_function_names: HashSet::new(),
            runtime_nominal_callable_names: HashSet::new(),
            current_input_runtime_nominal_names: HashSet::new(),
            runtime_nominal_constructor_indices: HashMap::new(),
            has_opaque_runtime_eval: false,
            function_ir_by_global_index: HashMap::new(),
            source_ordered_method_sigs: HashMap::new(),
            source_world_function_names: HashSet::new(),
            repl_source_ordered_dispatch: false,
            type_aliases: HashMap::new(),
            module_imported_bindings: HashMap::new(),
            module_publics: HashMap::new(),
            closure_captures: HashMap::new(),
            let_scope_global_closures: HashSet::new(),
            enum_types: HashMap::new(),
            primitive_types: Vec::new(),
            primitive_type_by_name: HashMap::new(),
        }
    }

    /// Register the user-declared primitive types (`primitive type Name Bits end`)
    /// so the compiler can resolve bare references to a `DataType` value and the
    /// runtime type-reflection layer can answer sizeof/supertype/isprimitivetype
    /// for them (Issue #5058). Later definitions of the same name win.
    pub fn set_primitive_types(&mut self, primitive_types: Vec<PrimitiveTypeDefInfo>) {
        self.primitive_type_by_name.clear();
        for (idx, pt) in primitive_types.iter().enumerate() {
            self.primitive_type_by_name.insert(pt.name.clone(), idx);
        }
        self.primitive_types = primitive_types;
    }

    /// Is `name` a user-declared primitive type?
    pub fn is_primitive_type_name(&self, name: &str) -> bool {
        self.primitive_type_by_name.contains_key(name)
    }

    /// Check if a user-defined struct satisfies a type bound.
    /// Walks the parent type chain to check if bound_name is an ancestor.
    pub fn check_struct_satisfies_bound(&self, struct_name: &str, bound_name: &str) -> bool {
        // If bound is "Any", everything satisfies it
        if bound_name == "Any" {
            return true;
        }

        // If the struct is the same as the bound, it satisfies it
        if bound_names_match(struct_name, bound_name) {
            return true;
        }

        // Find the struct in struct_defs
        let struct_def = self
            .struct_name_to_def_index
            .get(struct_name)
            .and_then(|idx| self.struct_defs.get(*idx));
        if let Some(def) = struct_def {
            if let Some(parent) = &def.parent_type {
                // Check if parent matches the bound. A module-local struct's
                // `parent_type` is stored module-qualified (`M.Ring`) while a
                // bound written in the same module scope (`R <: Ring`, or a
                // Union-alias member `RingElem`) stays bare, so compare modulo
                // the leading module prefix (Issue #8899).
                if bound_names_match(parent, bound_name) {
                    return true;
                }
                // Recursively check the parent's ancestors (abstract types)
                return self.check_abstract_type_satisfies_bound(parent, bound_name);
            }
        }

        // Also check if struct_name is an abstract type
        if self.check_abstract_type_satisfies_bound(struct_name, bound_name) {
            return true;
        }

        false
    }

    pub(crate) fn concrete_type_satisfies_bound(&self, jt: &JuliaType, bound_name: &str) -> bool {
        if check_type_satisfies_bound(jt, bound_name) {
            return true;
        }

        if let JuliaType::Struct(type_name) | JuliaType::AbstractUser(type_name, _) = jt {
            if self.check_struct_satisfies_bound(type_name, bound_name) {
                return true;
            }
        }

        self.expanded_bound_type(bound_name)
            .is_some_and(|bound| jt.is_subtype_of(&bound))
    }

    /// Check the reverse side of a type-parameter interval: `bound_name <: jt`.
    /// Constructor-self dispatch needs this for lower bounds (`T>:Integer`),
    /// complementing [`Self::concrete_type_satisfies_bound`] for upper bounds.
    pub(crate) fn bound_satisfies_concrete_type(&self, bound_name: &str, jt: &JuliaType) -> bool {
        let expanded_name = self
            .expand_type_aliases_in_type_name(bound_name, &mut HashSet::new())
            .unwrap_or_else(|| bound_name.to_string());
        self.type_name_satisfies_julia_bound(&expanded_name, jt)
            || JuliaType::from_name_or_struct(&expanded_name).is_subtype_of(jt)
    }

    pub(crate) fn type_name_satisfies_bound(&self, type_name: &str, bound_name: &str) -> bool {
        if self.check_struct_satisfies_bound(type_name, bound_name) {
            return true;
        }

        self.expanded_bound_type(bound_name).is_some_and(|bound| {
            self.type_name_satisfies_julia_bound(type_name, &bound)
                || JuliaType::from_name_or_struct(type_name).is_subtype_of(&bound)
        })
    }

    fn type_name_satisfies_julia_bound(&self, type_name: &str, bound: &JuliaType) -> bool {
        match bound {
            JuliaType::Union(members) => members
                .iter()
                .any(|member| self.type_name_satisfies_julia_bound(type_name, member)),
            JuliaType::Struct(name) | JuliaType::AbstractUser(name, _) => {
                self.check_struct_satisfies_bound(type_name, name)
            }
            _ => false,
        }
    }

    fn expanded_bound_type(&self, bound_name: &str) -> Option<JuliaType> {
        let expanded = self.expand_type_aliases_in_type_name(bound_name, &mut HashSet::new())?;
        (expanded != bound_name).then(|| JuliaType::from_name_or_struct(&expanded))
    }

    pub(crate) fn expand_type_param_bounds(&self, type_params: &[TypeParam]) -> Vec<TypeParam> {
        let excluded: HashSet<String> = type_params
            .iter()
            .map(|tp| type_param_base_name(&tp.name).to_string())
            .collect();
        type_params
            .iter()
            .map(|tp| {
                let mut expanded = tp.clone();
                expanded.upper_bound = tp
                    .get_upper_bound()
                    .and_then(|bound| {
                        self.expand_type_aliases_in_type_name_excluding(
                            bound,
                            &mut HashSet::new(),
                            &excluded,
                        )
                    })
                    .or_else(|| tp.get_upper_bound().cloned());
                expanded.bound = expanded.upper_bound.clone();
                expanded.lower_bound = tp.lower_bound.as_ref().map(|bound| {
                    self.expand_type_aliases_in_type_name_excluding(
                        bound,
                        &mut HashSet::new(),
                        &excluded,
                    )
                    .unwrap_or_else(|| bound.to_string())
                });
                expanded
            })
            .collect()
    }

    /// Canonicalize constructor `where` bounds in their lexical module before
    /// rebuilding the implicit constructor signature. This makes bare and
    /// qualified spellings of the same local bound identical while retaining
    /// exact owners for bounds from different modules (Issue #11019).
    pub(crate) fn expand_constructor_type_param_bounds(
        &self,
        type_params: &[TypeParam],
        module_path: Option<&str>,
    ) -> Vec<TypeParam> {
        let excluded: HashSet<String> = type_params
            .iter()
            .map(|tp| type_param_base_name(&tp.name).to_string())
            .collect();
        let canonicalize_bound = |bound: &str| {
            let expanded = self
                .expand_type_aliases_in_type_name_excluding_module(
                    bound,
                    &mut HashSet::new(),
                    &excluded,
                    module_path,
                )
                .unwrap_or_else(|| bound.to_string());
            parse_single_type_expr(&expanded)
                .map(|expression| {
                    self.qualify_constructor_self_type_expr(&expression, module_path, &excluded)
                        .to_string()
                })
                .unwrap_or(expanded)
        };

        type_params
            .iter()
            .map(|tp| {
                let mut expanded = tp.clone();
                expanded.upper_bound = tp.get_upper_bound().map(|bound| canonicalize_bound(bound));
                expanded.bound = expanded.upper_bound.clone();
                expanded.lower_bound = tp.lower_bound.as_deref().map(canonicalize_bound);
                expanded
            })
            .collect()
    }

    /// Expand user-visible aliases inside an explicit constructor self pattern.
    ///
    /// `Foo{Vector{S}}` and `Foo{MyVector{S}}` have the same implicit self when
    /// `const MyVector = Vector`. Constructor registration must therefore feed
    /// the same structured arguments to method-table dedup and runtime matching.
    /// Method-local binders are excluded so an unrelated alias named `S` cannot
    /// capture the constructor's `where S` variable (Issue #11019).
    pub(crate) fn expand_constructor_self_type_arguments(
        &self,
        arguments: &[TypeExpr],
        type_params: &[TypeParam],
        module_path: Option<&str>,
    ) -> Vec<TypeExpr> {
        let excluded: HashSet<String> = type_params
            .iter()
            .map(|tp| type_param_base_name(&tp.name).to_string())
            .collect();
        arguments
            .iter()
            .map(|argument| {
                let rendered = argument.to_string();
                self.expand_type_aliases_in_type_name_excluding_module(
                    &rendered,
                    &mut HashSet::new(),
                    &excluded,
                    module_path,
                )
                .and_then(|expanded| {
                    if expanded == rendered {
                        Some(argument.clone())
                    } else {
                        parse_single_type_expr(&expanded)
                    }
                })
                .map(|expanded| {
                    self.qualify_constructor_self_type_expr(&expanded, module_path, &excluded)
                })
                .unwrap_or_else(|| argument.clone())
            })
            .collect()
    }

    fn qualify_constructor_self_type_expr(
        &self,
        expression: &TypeExpr,
        module_path: Option<&str>,
        excluded: &HashSet<String>,
    ) -> TypeExpr {
        let qualify_name = |name: &str| {
            let path = module_path?;
            if name.contains('.') || excluded.contains(name) {
                return None;
            }
            let qualified = format!("{path}.{name}");
            (self.struct_table.contains_key(&qualified)
                || self.parametric_structs.contains_key(&qualified)
                || self.abstract_type_by_name.contains_key(&qualified)
                || self.primitive_type_by_name.contains_key(&qualified))
            .then_some(qualified)
        };

        match expression {
            TypeExpr::TypeVar(name) => qualify_name(name)
                .map(TypeExpr::TypeVar)
                .unwrap_or_else(|| expression.clone()),
            TypeExpr::Concrete(JuliaType::Struct(name)) => qualify_name(name)
                .map(|qualified| TypeExpr::Concrete(JuliaType::Struct(qualified)))
                .unwrap_or_else(|| expression.clone()),
            TypeExpr::Parameterized { base, params } => TypeExpr::Parameterized {
                base: qualify_name(base).unwrap_or_else(|| base.clone()),
                params: params
                    .iter()
                    .map(|param| {
                        self.qualify_constructor_self_type_expr(param, module_path, excluded)
                    })
                    .collect(),
            },
            TypeExpr::Concrete(_) | TypeExpr::RuntimeExpr(_) => expression.clone(),
        }
    }

    fn expand_type_aliases_in_type_name(
        &self,
        type_name: &str,
        seen: &mut HashSet<String>,
    ) -> Option<String> {
        self.expand_type_aliases_in_type_name_excluding(type_name, seen, &HashSet::new())
    }

    fn expand_type_aliases_in_type_name_excluding(
        &self,
        type_name: &str,
        seen: &mut HashSet<String>,
        excluded: &HashSet<String>,
    ) -> Option<String> {
        self.expand_type_aliases_in_type_name_excluding_module(type_name, seen, excluded, None)
    }

    fn expand_type_aliases_in_type_name_excluding_module(
        &self,
        type_name: &str,
        seen: &mut HashSet<String>,
        excluded: &HashSet<String>,
        module_path: Option<&str>,
    ) -> Option<String> {
        let type_name = type_name.trim();
        if type_name.is_empty() {
            return Some(type_name.to_string());
        }

        if !bound_alias_name_is_excluded(type_name, excluded) {
            if let Some(target) = self.resolve_type_alias_in_module(type_name, module_path) {
                if !seen.insert(type_name.to_string()) {
                    return Some(type_name.to_string());
                }
                let expanded = self.expand_type_aliases_in_type_name_excluding_module(
                    &target,
                    seen,
                    excluded,
                    module_path,
                );
                seen.remove(type_name);
                return expanded;
            }
        }

        if let Some(inner) = type_name
            .strip_prefix("Union{")
            .and_then(|s| s.strip_suffix('}'))
        {
            let args = parse_type_args_recursive(inner)?;
            let expanded_args = args
                .iter()
                .map(|arg| {
                    self.expand_type_aliases_in_type_name_excluding_module(
                        &arg.to_string(),
                        seen,
                        excluded,
                        module_path,
                    )
                })
                .collect::<Option<Vec<_>>>()?;
            return Some(format!("Union{{{}}}", expanded_args.join(", ")));
        }

        if let Some((base, params)) = parse_parametric_call(type_name) {
            let expanded_base = self
                .expand_type_aliases_in_type_name_excluding_module(
                    &base,
                    seen,
                    excluded,
                    module_path,
                )
                .unwrap_or(base);
            let expanded_params = params
                .iter()
                .map(|param| {
                    self.expand_type_aliases_in_type_name_excluding_module(
                        &param.to_string(),
                        seen,
                        excluded,
                        module_path,
                    )
                })
                .collect::<Option<Vec<_>>>()?;
            return Some(format!(
                "{}{{{}}}",
                expanded_base,
                expanded_params.join(", ")
            ));
        }

        Some(type_name.to_string())
    }

    fn resolve_type_alias_in_module(
        &self,
        type_name: &str,
        module_path: Option<&str>,
    ) -> Option<String> {
        if !type_name.contains('.') {
            let mut lexical_path = module_path;
            while let Some(path) = lexical_path {
                let qualified = format!("{path}.{type_name}");
                if let Some(target) = self.type_aliases.get(&qualified) {
                    return Some(target.clone());
                }
                lexical_path = path.rsplit_once('.').map(|(parent, _)| parent);
            }
        }
        self.resolve_bound_type_alias(type_name)
    }

    fn resolve_bound_type_alias(&self, type_name: &str) -> Option<String> {
        if let Some(target) = self.type_aliases.get(type_name) {
            return Some(target.clone());
        }

        let mut unique_target: Option<&String> = None;
        for (alias, target) in &self.type_aliases {
            if alias.rsplit('.').next() != Some(type_name) {
                continue;
            }
            if unique_target.is_some_and(|existing| existing != target) {
                return None;
            }
            unique_target = Some(target);
        }
        unique_target.cloned()
    }

    /// Check if an abstract type satisfies a bound by walking the parent chain.
    pub fn check_abstract_type_satisfies_bound(&self, type_name: &str, bound_name: &str) -> bool {
        // If they match, it satisfies (modulo a leading module prefix so a
        // qualified `M.Ring` matches a bare bound `Ring`, Issue #8899).
        if bound_names_match(type_name, bound_name) {
            return true;
        }

        // Find the abstract type in abstract_types
        if let Some(at) = self
            .abstract_type_by_name
            .get(type_name)
            .and_then(|idx| self.abstract_types.get(*idx))
        {
            if let Some(parent) = &at.parent {
                // Check if parent matches the bound (same qualified-vs-bare
                // normalization as above, Issue #8899).
                if bound_names_match(parent, bound_name) {
                    return true;
                }
                // Recursively check the parent
                return self.check_abstract_type_satisfies_bound(parent, bound_name);
            }
        }

        false
    }

    /// Look up struct name by type_id.
    pub fn get_struct_name(&self, type_id: usize) -> Option<String> {
        self.type_id_to_struct_name.get(&type_id).cloned()
    }

    /// Look up the precise per-field `JuliaType` list for a struct by type_id.
    ///
    /// Unlike `StructInfo::fields` (whose `ValueType` collapses every
    /// signed/unsigned integer width to `I64`), this preserves the declared
    /// field widths such as `UInt64` / `Int32`, which constructor-argument
    /// coercion needs to avoid clobbering them (Issue #4990).
    pub fn field_julia_types_by_type_id(&self, type_id: usize) -> Option<&[JuliaType]> {
        let name = self.type_id_to_struct_name.get(&type_id)?;
        let idx = self.struct_name_to_def_index.get(name)?;
        self.struct_defs
            .get(*idx)
            .map(|def| def.field_julia_types.as_slice())
    }

    /// Resolve (or create) a parametric type instantiation.
    /// Returns the type_id for the concrete instantiation.
    pub fn resolve_instantiation(
        &mut self,
        base_name: &str,
        type_args: &[JuliaType],
    ) -> CResult<usize> {
        // Convert JuliaType to TypeExpr and delegate
        let type_exprs: Vec<TypeExpr> = type_args
            .iter()
            .map(|jt| TypeExpr::Concrete(jt.clone()))
            .collect();
        self.resolve_instantiation_with_type_expr(base_name, &type_exprs)
    }

    /// Resolve (or create) a parametric type instantiation using TypeExpr.
    /// Returns the type_id for the concrete instantiation.
    /// Supports nested parameterized types like Container{Point{Float64}}.
    pub fn resolve_instantiation_with_type_expr(
        &mut self,
        base_name: &str,
        type_args: &[TypeExpr],
    ) -> CResult<usize> {
        let explicit_base_name = base_name.strip_prefix("Base.");
        let explicit_base_def =
            explicit_base_name.and_then(|base| self.base_parametric_structs.get(base));
        // `Base.T` selects the private Base definition, but Base's top-level
        // structs still have the canonical concrete identity `T{...}`. Keeping
        // the owner only in the definition lookup prevents a second
        // `Base.T{...}` type-id family from being minted (Issue #11369).
        let canonical_base_name = if let Some(base) = explicit_base_name.filter(|base| {
            explicit_base_def.is_some() || self.parametric_structs.contains_key(*base)
        }) {
            base.to_string()
        } else {
            self.canonical_parametric_base_name(base_name)
        };
        let key = InstantiationKey {
            base_name: canonical_base_name.clone(),
            type_args: type_args.to_vec(),
        };

        // Check if already instantiated
        if let Some(&type_id) = self.instantiation_table.get(&key) {
            return Ok(type_id);
        }

        // Check if any type_arg is a type variable - if so, we cannot instantiate
        // Type variables should only be used for method dispatch matching, not for creating concrete instances
        // NOTE: Pure numeric strings (like "5" in Val{5}) are VALUE parameters, not type variables
        for arg in type_args.iter() {
            if let TypeExpr::TypeVar(type_name) = arg {
                // Pure numeric strings are value parameters, not type variables
                if type_name.chars().all(|c| c.is_ascii_digit()) {
                    continue; // Not a type variable, just a value parameter
                }
                let is_type_variable = type_name.len() <= 2
                    && type_name
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
                if is_type_variable {
                    return err(format!(
                        "Cannot instantiate parametric type {}{{{}}} with type variable {}",
                        base_name,
                        TypeExpr::render_param_list(type_args),
                        type_name
                    ));
                }
            }
        }

        // Get the parametric struct definition
        let parametric_def = explicit_base_def
            .or_else(|| self.parametric_structs.get(&canonical_base_name))
            .ok_or_else(|| {
                CompileError::Msg(format!("Unknown parametric struct: {}", base_name))
            })?;
        let def = parametric_def.def.clone();
        let declaring_owner = self.parametric_declaring_owner(&canonical_base_name, &def);

        // Check type bounds
        if type_args.len() != def.type_params.len() {
            return err(format!(
                "{}{{...}} expects {} type parameters, got {}",
                base_name,
                def.type_params.len(),
                type_args.len()
            ));
        }

        // Check type bounds
        for (param, arg) in def.type_params.iter().zip(type_args.iter()) {
            if let Some(bound_name) = &param.bound {
                match arg {
                    TypeExpr::Concrete(jt) => {
                        // Built-in types: use check_type_satisfies_bound
                        if !self.concrete_type_satisfies_bound(jt, bound_name) {
                            return err(format!(
                                "Type {} does not satisfy bound {}<:{}",
                                jt.name(),
                                param.name,
                                bound_name
                            ));
                        }
                    }
                    TypeExpr::TypeVar(type_name) => {
                        // Check if this is a type variable from a where clause (e.g., T, S, R)
                        // Type variables are typically single uppercase letters or short names
                        // We should skip bound checking for these - they will be checked at instantiation
                        let is_type_variable = type_name.len() <= 2
                            && type_name
                                .chars()
                                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());

                        if is_type_variable {
                            // Skip bound checking for type variables - will be checked when concrete type is used
                            continue;
                        }

                        // User-defined struct/type name: use check_struct_satisfies_bound
                        if !self.type_name_satisfies_bound(type_name, bound_name) {
                            return err(format!(
                                "Type {} does not satisfy bound {}<:{}",
                                type_name, param.name, bound_name
                            ));
                        }
                    }
                    TypeExpr::Parameterized { .. } => {
                        // For parameterized types, we skip bound checking here
                        // (the nested type will be checked when instantiated)
                    }
                    TypeExpr::RuntimeExpr(_) => {
                        // Runtime expressions are evaluated at runtime - skip bound checking here
                    }
                }
            }
        }

        // Build type parameter substitution map (TypeExpr-based for nested type support)
        let type_subst: HashMap<String, TypeExpr> = def
            .type_params
            .iter()
            .zip(type_args.iter())
            .map(|(p, a)| (p.name.clone(), a.clone()))
            .collect();

        // Substitute type parameters in fields (using recursive substitution for nested types)
        let mut fields: Vec<(String, ValueType)> = Vec::new();
        let mut field_julia_types: Vec<JuliaType> = Vec::new();
        for f in &def.fields {
            let vt = self.substitute_field_type(&f.type_expr, &type_subst, def.is_base_origin)?;
            fields.push((f.name.clone(), vt));
            let jt = f
                .type_expr
                .as_ref()
                .map(|type_expr| self.resolve_type_expr_recursive(type_expr, &type_subst))
                .transpose()?
                .as_ref()
                .map(TypeExpr::to_julia_type_lossy)
                .unwrap_or(JuliaType::Any);
            field_julia_types.push(jt);
        }

        // Build instantiated name for display (e.g., "Point{Float64}" or "Container{Point{Float64}}")
        let instantiated_name = TypeExpr::format_parameterized(&canonical_base_name, type_args);

        // All parametric structs (including Complex) get sequential type_ids
        let type_id = self.next_type_id;
        self.next_type_id += 1;

        // Register in struct_table
        self.struct_table.insert_owned(
            instantiated_name.clone(),
            &declaring_owner,
            StructInfo {
                type_id,
                is_mutable: def.is_mutable,
                fields: fields.clone(),
                has_inner_constructor: !def.inner_constructors.is_empty(),
            },
        );
        // Keep the parametric-prefix index (Issue #10129) in sync with this
        // new instantiation so `resolve_global_types()`'s O(1) lookup sees it.
        if let Some(brace_idx) = instantiated_name.find('{') {
            self.parametric_struct_prefix_index
                .insert(instantiated_name[..=brace_idx].to_string(), type_id);
        }

        // Register in struct_defs (all structs including Complex need this for name lookup)
        self.struct_defs.push(StructDefInfo {
            name: instantiated_name.clone(),
            is_mutable: def.is_mutable,
            fields,
            field_julia_types,
            parent_type: def.parent_type.clone(),
        });
        self.struct_name_to_def_index
            .insert(instantiated_name.clone(), self.struct_defs.len() - 1);
        self.type_id_to_struct_name
            .insert(type_id, instantiated_name.clone());

        // Cache the instantiation
        self.instantiation_table.insert(key, type_id);

        Ok(type_id)
    }

    /// Preserve nominal owner identity when a bare parametric alias is
    /// ambiguous across modules.
    ///
    /// Module structs are registered under both their qualified declaration
    /// name and a source-visible bare alias. The bare alias is intentionally
    /// last-wins, but using that spelling for a concrete instantiation loses
    /// the selected declaration's owner. That is observable when another
    /// module declares the same family: `M.T{A}` must not become the unrelated
    /// `N.T{A}` during dispatch. In that collision domain, recover the unique
    /// qualified key whose definition is structurally identical to the alias.
    /// Outside a real collision, retain the historical bare spelling.
    fn canonical_parametric_base_name(&self, base_name: &str) -> String {
        // A const alias to a parametric type owned by another module
        // (`module OwnerB; const Y = OwnerA.X; end` -> `OwnerB.Y{Int}`,
        // Issue #11068) resolves through the alias table BEFORE the dotted
        // early-return below, chasing alias-to-alias chains with a small
        // bound. Unqualified aliases already resolve on other paths; this
        // covers the qualified spelling the dotted return used to swallow.
        if !self.parametric_structs.contains_key(base_name) {
            let mut current = base_name;
            for _ in 0..8 {
                let Some(target) = self.type_aliases.get(current) else {
                    break;
                };
                if self.parametric_structs.contains_key(target.as_str()) {
                    return target.clone();
                }
                current = target;
            }
        }
        if base_name.contains('.') {
            return base_name.to_string();
        }
        let Some(alias) = self.parametric_structs.get(base_name) else {
            return base_name.to_string();
        };

        let qualified: Vec<(&String, &ParametricStructDef)> = self
            .parametric_structs
            .iter()
            .filter(|(name, _)| name.contains('.') && name.rsplit('.').next() == Some(base_name))
            .collect();
        if qualified.is_empty()
            || (qualified.len() == 1
                && !crate::types::has_qualified_nominal_family_collision(base_name))
        {
            return base_name.to_string();
        }

        let mut owners = qualified
            .into_iter()
            .filter(|(_, candidate)| candidate.def == alias.def)
            .map(|(name, _)| name.as_str());
        match (owners.next(), owners.next()) {
            (Some(owner), None) => owner.to_string(),
            _ => base_name.to_string(),
        }
    }

    fn parametric_declaring_owner(
        &self,
        canonical_base_name: &str,
        def: &crate::ir::core::StructDef,
    ) -> String {
        if let Some((owner, _)) = canonical_base_name.rsplit_once('.') {
            return owner.to_string();
        }

        let suffix = format!(".{canonical_base_name}");
        let mut owners = self
            .parametric_structs
            .iter()
            .filter(|(name, candidate)| name.ends_with(&suffix) && candidate.def == *def)
            .filter_map(|(name, _)| name.strip_suffix(&suffix));
        match (owners.next(), owners.next()) {
            (Some(owner), None) => owner.to_string(),
            _ => "Main".to_string(),
        }
    }

    /// Infer type arguments from constructor arguments for a parametric struct.
    pub fn infer_type_args(
        &self,
        base_name: &str,
        arg_types: &[JuliaType],
    ) -> CResult<Vec<JuliaType>> {
        let parametric_def = self.parametric_structs.get(base_name).ok_or_else(|| {
            CompileError::Msg(format!("Unknown parametric struct: {}", base_name))
        })?;
        infer_parametric_type_args(&parametric_def.def, base_name, arg_types)
    }

    /// Structurally match a field's declared `TypeExpr` against the actual
    /// `JuliaType` of the corresponding constructor argument, binding any
    /// embedded type variables named in `param_names`.
    ///
    /// Examples:
    /// - `T` vs `Int64`              => binds T = Int64
    /// - `Tuple{T,T}` vs `Tuple{Int64,Int64}` => binds T = Int64
    /// - `Array{T}` / `Vector{T}` vs `Vector{Int64}` => binds T = Int64
    /// - `Foo{T}` vs `Foo{Int64}`    => binds T = Int64 (parametric struct)
    ///
    /// When the shapes do not align (e.g. the actual type is `Any` or differs
    /// structurally), embedded variables are simply left unbound; the caller's
    /// final per-parameter lookup reports any parameter that stayed unbound.
    #[cfg(test)]
    fn bind_type_vars_from_expr(
        type_expr: &TypeExpr,
        actual: &JuliaType,
        param_names: &[&str],
        inferred: &mut HashMap<String, JuliaType>,
    ) -> CResult<()> {
        parametric::bind_type_vars_from_expr(type_expr, actual, param_names, inferred)
            .map_err(CompileError::Msg)
    }

    /// Substitute type parameters in a field type and convert to ValueType.
    /// Handles nested parameterized types like Array{T} or Point{Float64}.
    pub fn substitute_field_type(
        &mut self,
        type_expr: &Option<TypeExpr>,
        type_subst: &HashMap<String, TypeExpr>,
        base_origin_owner: bool,
    ) -> CResult<ValueType> {
        match type_expr {
            None => Ok(ValueType::Any), // Untyped fields are Any (Julia semantics)
            Some(TypeExpr::Concrete(jt)) => {
                // Handle JuliaType::Struct specially - look up type_id from struct_table
                match jt {
                    JuliaType::Struct(name) => {
                        if let Some((_, info)) =
                            self.struct_table
                                .resolve_scoped(name, None, base_origin_owner)
                        {
                            Ok(ValueType::Struct(info.type_id))
                        } else {
                            // Struct not yet defined, fallback to Any
                            Ok(ValueType::Any)
                        }
                    }
                    // Abstract numeric fields keep an Any storage tag so the
                    // original runtime value survives (Issue #11407).
                    _ => Ok(crate::compile::type_helpers::field_declared_value_type(jt)),
                }
            }
            Some(TypeExpr::TypeVar(name)) => {
                if let Some(substituted) = type_subst.get(name) {
                    // Check for self-referential substitution (e.g., T -> TypeVar("T"))
                    // This happens when function parameters have types like Box{T} where T
                    // is a type variable from the where clause, not a concrete type.
                    if let TypeExpr::TypeVar(sub_name) = substituted {
                        if sub_name == name {
                            // Self-referential: T -> T, return Any since concrete type is unknown
                            return Ok(ValueType::Any);
                        }
                    }
                    // Recursively substitute
                    self.substitute_field_type(
                        &Some(substituted.clone()),
                        type_subst,
                        base_origin_owner,
                    )
                } else {
                    // Not in type_subst - check if it's a known struct or type name
                    if let Some((_, info)) =
                        self.struct_table
                            .resolve_scoped(name, None, base_origin_owner)
                    {
                        Ok(ValueType::Struct(info.type_id))
                    } else if let Some(jt) = JuliaType::from_name(name) {
                        Ok(julia_type_to_value_type(&jt))
                    } else {
                        Ok(ValueType::F64) // Default for truly unbound type vars
                    }
                }
            }
            Some(TypeExpr::Parameterized { base, params }) => {
                // Recursively resolve nested parameterized type
                // First, substitute type parameters in the params
                let resolved_params: Vec<TypeExpr> = params
                    .iter()
                    .map(|p| self.resolve_type_expr_recursive(p, type_subst))
                    .collect::<CResult<_>>()?;

                // Special case: Array/Vector are not user-defined structs
                if base == "Array" || base == "Vector" {
                    return Ok(ValueType::Array);
                }

                // Special case: Memory{T} is the native Memory primitive, not a
                // user struct. Mirrors the Array/Vector arm above: return the
                // generic `Memory` ValueType (a `MemoryOf(_)` value coerces to it
                // as a no-op), so a parametric struct field `keys::Memory{K}`
                // resolves correctly instead of falling through to the F64
                // "unknown base type" default below (Issue #6623). The struct's
                // own `K`/`V` parameters still bind via the instantiation
                // mechanism, exactly as for `Vector{K}` fields.
                if base == "Memory" || base == "MemoryRef" {
                    return Ok(ValueType::Memory);
                }

                // Special case: Tuple{...} fields are stored as tuple values.
                // (e.g. a field `a::Tuple{T,T}` holds a Tuple value, not a struct.)
                if base == "Tuple" {
                    return Ok(ValueType::Tuple);
                }

                // Check if any resolved param is still a type variable or runtime expr
                // If so, we can't create a concrete instantiation - return Any
                let has_type_var = resolved_params.iter().any(|p| {
                    match p {
                        TypeExpr::TypeVar(name) => {
                            name.len() <= 2
                                && name
                                    .chars()
                                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                        }
                        TypeExpr::RuntimeExpr(_) => true, // Runtime expressions can't be resolved at compile time
                        _ => false,
                    }
                });
                if has_type_var {
                    return Ok(ValueType::Any);
                }

                // A Base-origin definition owns bare nested type names in its
                // field declarations. For example, the `Dict{T,Nothing}` field
                // of `Base.Set{T}` must not bind to a user module's later bare
                // `Dict` alias (Issue #11369).
                let owned_base = if base_origin_owner && !base.contains('.') {
                    let qualified = format!("Base.{}", base);
                    self.base_parametric_structs
                        .contains_key(base)
                        .then_some(qualified)
                } else {
                    None
                };
                let resolved_base = owned_base.as_deref().unwrap_or(base);

                // Check if this is a known parametric struct
                if owned_base.is_some() || self.parametric_structs.contains_key(resolved_base) {
                    let type_id =
                        self.resolve_instantiation_with_type_expr(resolved_base, &resolved_params)?;
                    return Ok(ValueType::Struct(type_id));
                }

                // Unknown base type, default to F64
                Ok(ValueType::F64)
            }
            Some(TypeExpr::RuntimeExpr(_)) => {
                // Runtime expressions can't be resolved at compile time - return Any
                Ok(ValueType::Any)
            }
        }
    }

    /// Resolve a type expression by substituting type variables.
    /// Returns a new TypeExpr with substitutions applied.
    pub fn resolve_type_expr_recursive(
        &self,
        expr: &TypeExpr,
        type_subst: &HashMap<String, TypeExpr>,
    ) -> CResult<TypeExpr> {
        match expr {
            TypeExpr::Concrete(_) => Ok(expr.clone()),
            TypeExpr::TypeVar(name) => {
                if let Some(substituted) = type_subst.get(name) {
                    Ok(substituted.clone())
                } else {
                    // Unbound type var, keep as is
                    Ok(expr.clone())
                }
            }
            TypeExpr::Parameterized { base, params } => {
                let resolved_params: Vec<TypeExpr> = params
                    .iter()
                    .map(|p| self.resolve_type_expr_recursive(p, type_subst))
                    .collect::<CResult<_>>()?;
                Ok(TypeExpr::Parameterized {
                    base: base.clone(),
                    params: resolved_params,
                })
            }
            TypeExpr::RuntimeExpr(_) => {
                // Runtime expressions can't be substituted - keep as is
                Ok(expr.clone())
            }
        }
    }

    /// Check if a struct name matches the given base name (e.g., "Complex" matches "Complex{Float64}")
    pub fn is_struct_of_base(&self, name: &str, base_name: &str) -> bool {
        name == base_name || name.starts_with(&format!("{}{{", base_name))
    }

    /// Check if a ValueType represents a struct with the given base name
    pub fn is_struct_type_of(&self, ty: &ValueType, base_name: &str) -> bool {
        if let ValueType::Struct(type_id) = ty {
            if let Some(def) = self.struct_defs.get(*type_id) {
                return self.is_struct_of_base(&def.name, base_name);
            }
            for (name, info) in &self.struct_table {
                if info.type_id == *type_id {
                    return self.is_struct_of_base(name, base_name);
                }
            }
        }
        false
    }

    /// Resolve the type id of one exactly named struct declaration.
    ///
    /// A family name is not a concrete identity. Callers that know type
    /// parameters must resolve the complete instantiation first; callers that
    /// do not must remain dynamic rather than selecting a hash-backed same-base
    /// entry (Issue #11436).
    pub fn get_struct_type_id(&self, name: &str) -> Option<usize> {
        self.struct_table
            .resolve(name)
            .map(|(_, info)| info.type_id)
    }

    /// Resolve only an exactly named concrete struct for flow-sensitive
    /// narrowing.
    ///
    /// A bare parametric family such as `SubArray` has many instantiated
    /// entries in `instantiation_table`; choosing the first HashMap entry
    /// would narrow `x isa SubArray` to an arbitrary concrete instantiation
    /// and make subsequent static dispatch process-order dependent. Bare
    /// parametric guards must therefore remain `Any` until the runtime value
    /// supplies its parameters (Issue #11264).
    pub fn get_exact_struct_type_id(&self, name: &str) -> Option<usize> {
        self.get_struct_type_id(name)
    }
}

/// Infer the concrete type arguments of a parametric struct from the actual
/// `JuliaType`s of its default-constructor arguments.
///
/// This is the context-free core shared by [`SharedCompileContext::infer_type_args`]
/// and the reflection-time inference engine (Issues #4849 / #4850 / #4851). It
/// structurally matches each declared field `TypeExpr` against the corresponding
/// argument type, binding the struct's declared type parameters — including ones
/// embedded inside nested field types such as `Tuple{T,T}` or `Vector{T}`.
pub fn infer_parametric_type_args(
    def: &crate::ir::core::StructDef,
    base_name: &str,
    arg_types: &[JuliaType],
) -> CResult<Vec<JuliaType>> {
    parametric::infer_parametric_type_args(def, base_name, arg_types).map_err(CompileError::Msg)
}

/// Parse the type parameters of a parametric struct instantiation name.
///
/// Given an instantiated struct name like `"Foo{Int64, String}"` and the
/// expected base `"Foo"`, returns `Some(vec![Int64, String])`. Returns `None`
/// if the name's base does not match `base`, or if the name carries no
/// parameters.
#[cfg(test)]
fn parse_struct_type_params(name: &str, base: &str) -> Option<Vec<JuliaType>> {
    parametric::parse_struct_type_params(name, base)
}

fn type_param_base_name(name: &str) -> &str {
    name.split_once("<:")
        .or_else(|| name.split_once(">:"))
        .map_or(name, |(base, _)| base)
        .trim()
}

fn bound_alias_name_is_excluded(type_name: &str, excluded: &HashSet<String>) -> bool {
    excluded.contains(type_name)
}

/// Two type-name strings denote the same type for a compile-time bound check,
/// treating a leading module qualification as insensitive (Issue #8899).
///
/// A module-local struct/abstract type's `parent_type` is stored
/// module-qualified (`M.Ring`), but a bound written in the same module scope
/// — a parametric type-parameter bound (`struct Poly{R <: Ring}` in module
/// `M`) or a member produced by expanding a `Union` type alias
/// (`RingElement = Union{RingElem, ...}`) — stays bare (`Ring`, `RingElem`).
/// Comparing the two raw strings then wrongly rejects a valid construction
/// such as `M.Poly{M.BaseRing}(...)`. The exact-equality path is always tried
/// first by callers; this fallback fires only when at least one side carries a
/// module prefix, so it never loosens a genuine bare-vs-bare mismatch. The
/// parametric-parameter tail (`{...}`) is preserved and compared verbatim so
/// only the leading module path is normalized away.
fn bound_names_match(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    if (!a.contains('.') && !b.contains('.')) || (a.contains('.') && b.contains('.')) {
        return false;
    }
    fn strip_leading_module(name: &str) -> String {
        let (base, params) = match name.find('{') {
            Some(idx) => (&name[..idx], &name[idx..]),
            None => (name, ""),
        };
        let base = base.rsplit('.').next().unwrap_or(base);
        format!("{}{}", base, params)
    }
    strip_leading_module(a) == strip_leading_module(b)
}

#[cfg(test)]
mod type_definition_position_tests {
    use super::TypeDefinitionPosition;

    #[test]
    fn type_definition_position_prefers_ordinals_when_both_exist_issue_11117() {
        let definition = TypeDefinitionPosition {
            definition_order: 4,
            source_start: 900,
        };
        assert!(definition.is_before(5, 100));
        assert!(!definition.is_before(3, 1000));
    }

    #[test]
    fn type_definition_position_uses_offsets_for_zero_ordinals_issue_11117() {
        let earlier = TypeDefinitionPosition {
            definition_order: 9,
            source_start: 100,
        };
        let later = TypeDefinitionPosition {
            definition_order: 0,
            source_start: 300,
        };
        assert!(earlier.is_before(0, 200));
        assert!(!later.is_before(12, 200));
    }

    #[test]
    fn type_definition_position_uses_offsets_for_lowering_helpers_11685() {
        let earlier = TypeDefinitionPosition {
            definition_order: 9,
            source_start: 100,
        };
        assert!(earlier.is_before(crate::ir::core::LOWERING_HELPER_DEFINITION_ORDER, 200));
        assert!(!earlier.is_before(crate::ir::core::LOWERING_HELPER_DEFINITION_ORDER, 50));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::bytecode::{AbstractTypeDefInfo, StructDefInfo, ValueType};

    /// Helper: build a minimal SharedCompileContext with the given structs and abstract types.
    fn make_ctx(
        structs: Vec<(&str, usize, Option<&str>)>, // (name, type_id, parent)
        abstract_types: Vec<(&str, Option<&str>)>, // (name, parent)
    ) -> SharedCompileContext {
        let mut struct_table = StructRegistry::new();
        let mut struct_defs = Vec::new();
        for (name, type_id, parent) in &structs {
            struct_table.insert(
                name.to_string(),
                StructInfo {
                    type_id: *type_id,
                    is_mutable: false,
                    fields: vec![],
                    has_inner_constructor: false,
                },
            );
            struct_defs.push(StructDefInfo {
                name: name.to_string(),
                is_mutable: false,
                fields: vec![("x".to_string(), ValueType::F64)],
                field_julia_types: vec![JuliaType::Float64],
                parent_type: parent.map(|s| s.to_string()),
            });
        }
        let abs_types: Vec<AbstractTypeDefInfo> = abstract_types
            .iter()
            .map(|(name, parent)| AbstractTypeDefInfo {
                name: name.to_string(),
                parent: parent.map(|s| s.to_string()),
                type_params: vec![],
            })
            .collect();
        SharedCompileContext::new(
            struct_table,
            struct_defs,
            HashMap::new(),
            HashMap::new(),
            abs_types,
            structs.len(),
        )
    }

    // ── check_struct_satisfies_bound ─────────────────────────────────────────

    #[test]
    fn test_bound_any_always_satisfied() {
        let ctx = make_ctx(vec![("Dog", 0, Some("Animal"))], vec![("Animal", None)]);
        assert!(ctx.check_struct_satisfies_bound("Dog", "Any"));
        assert!(ctx.check_struct_satisfies_bound("UnknownType", "Any"));
    }

    #[test]
    fn test_expand_type_param_bounds_uses_qualified_alias_leaf_issue_8406() {
        let mut ctx = make_ctx(vec![], vec![]);
        ctx.type_aliases.insert(
            "AbstractAlgebra.RingElement".to_string(),
            "Union{RingElem, Integer, Rational, AbstractFloat}".to_string(),
        );
        let expanded = ctx.expand_type_param_bounds(&[TypeParam::with_upper_bound(
            "T".to_string(),
            "RingElement".to_string(),
        )]);
        assert_eq!(expanded.len(), 1);
        assert_eq!(
            expanded[0].get_upper_bound().map(String::as_str),
            Some("Union{RingElem, Integer, Rational, AbstractFloat}")
        );
        assert_eq!(
            expanded[0].bound.as_deref(),
            Some("Union{RingElem, Integer, Rational, AbstractFloat}")
        );
    }

    #[test]
    fn test_bound_same_name() {
        let ctx = make_ctx(vec![("Dog", 0, None)], vec![]);
        assert!(ctx.check_struct_satisfies_bound("Dog", "Dog"));
    }

    #[test]
    fn test_bound_direct_parent() {
        let ctx = make_ctx(vec![("Dog", 0, Some("Animal"))], vec![("Animal", None)]);
        assert!(ctx.check_struct_satisfies_bound("Dog", "Animal"));
    }

    #[test]
    fn test_bound_transitive_ancestor() {
        // Dog <: Mammal <: Animal
        let ctx = make_ctx(
            vec![("Dog", 0, Some("Mammal"))],
            vec![("Mammal", Some("Animal")), ("Animal", None)],
        );
        assert!(ctx.check_struct_satisfies_bound("Dog", "Animal"));
        assert!(ctx.check_struct_satisfies_bound("Dog", "Mammal"));
    }

    #[test]
    fn test_bound_unrelated_struct() {
        let ctx = make_ctx(
            vec![("Dog", 0, Some("Animal")), ("Cat", 1, Some("Animal"))],
            vec![("Animal", None)],
        );
        // Dog does NOT satisfy Cat bound
        assert!(!ctx.check_struct_satisfies_bound("Dog", "Cat"));
    }

    #[test]
    fn test_bound_unknown_struct_returns_false() {
        let ctx = make_ctx(vec![], vec![]);
        assert!(!ctx.check_struct_satisfies_bound("Unknown", "SomeBound"));
    }

    #[test]
    fn test_bound_names_match_normalizes_leading_module_issue_8899() {
        // Qualified parent vs bare bound (and vice versa) match.
        assert!(bound_names_match("M.Ring", "Ring"));
        assert!(bound_names_match("Ring", "M.Ring"));
        assert!(bound_names_match("A.B.Ring", "Ring"));
        // Exact equality still matches.
        assert!(bound_names_match("Ring", "Ring"));
        // Bare-vs-bare mismatch is NOT loosened (fallback only fires with a dot).
        assert!(!bound_names_match("Ring", "Field"));
        // Parametric tail is preserved: only the leading module is stripped.
        assert!(bound_names_match("M.Poly{T}", "Poly{T}"));
        assert!(!bound_names_match("M.Poly{Int}", "Poly{Str}"));
    }

    #[test]
    fn test_bound_module_qualified_parent_matches_bare_bound_issue_8899() {
        // struct M.BaseRing <: M.Ring; the parametric bound is written bare as
        // `Ring` (module-local scope) while the stored parent_type is `M.Ring`.
        let ctx = make_ctx(
            vec![("M.BaseRing", 0, Some("M.Ring"))],
            vec![("M.Ring", None)],
        );
        assert!(ctx.check_struct_satisfies_bound("M.BaseRing", "Ring"));
        // A genuinely-unrelated bare bound still fails.
        assert!(!ctx.check_struct_satisfies_bound("M.BaseRing", "Field"));
    }

    #[test]
    fn test_bound_module_qualified_union_alias_member_issue_8409() {
        // struct M.Poly <: M.RingElem; the union-alias member expands to bare
        // `RingElem` but the stored parent_type is `M.RingElem`.
        let ctx = make_ctx(
            vec![("M.Poly", 0, Some("M.RingElem"))],
            vec![("M.RingElem", None)],
        );
        assert!(ctx.check_struct_satisfies_bound("M.Poly", "RingElem"));
    }

    // ── check_abstract_type_satisfies_bound ──────────────────────────────────

    #[test]
    fn test_abstract_satisfies_itself() {
        let ctx = make_ctx(vec![], vec![("Animal", None)]);
        assert!(ctx.check_abstract_type_satisfies_bound("Animal", "Animal"));
    }

    #[test]
    fn test_abstract_satisfies_parent() {
        let ctx = make_ctx(vec![], vec![("Mammal", Some("Animal")), ("Animal", None)]);
        assert!(ctx.check_abstract_type_satisfies_bound("Mammal", "Animal"));
    }

    #[test]
    fn test_abstract_transitive() {
        let ctx = make_ctx(
            vec![],
            vec![
                ("Dog", Some("Mammal")),
                ("Mammal", Some("Animal")),
                ("Animal", None),
            ],
        );
        assert!(ctx.check_abstract_type_satisfies_bound("Dog", "Animal"));
    }

    #[test]
    fn test_abstract_unrelated_returns_false() {
        let ctx = make_ctx(vec![], vec![("Animal", None), ("Plant", None)]);
        assert!(!ctx.check_abstract_type_satisfies_bound("Animal", "Plant"));
    }

    // ── get_struct_name ──────────────────────────────────────────────────────

    #[test]
    fn test_get_struct_name_known_type_id() {
        let ctx = make_ctx(vec![("Point", 7, None)], vec![]);
        assert_eq!(ctx.get_struct_name(7), Some("Point".to_string()));
    }

    #[test]
    fn test_get_struct_name_unknown_type_id() {
        let ctx = make_ctx(vec![("Point", 7, None)], vec![]);
        assert_eq!(ctx.get_struct_name(999), None);
    }

    #[test]
    fn test_get_struct_name_multiple_structs() {
        let ctx = make_ctx(
            vec![("Point", 0, None), ("Circle", 1, None), ("Rect", 2, None)],
            vec![],
        );
        assert_eq!(ctx.get_struct_name(0), Some("Point".to_string()));
        assert_eq!(ctx.get_struct_name(1), Some("Circle".to_string()));
        assert_eq!(ctx.get_struct_name(2), Some("Rect".to_string()));
    }

    // ── bind_type_vars_from_expr (Issue #4851) ───────────────────────────────

    fn tv(name: &str) -> TypeExpr {
        TypeExpr::TypeVar(name.to_string())
    }

    fn param(base: &str, params: Vec<TypeExpr>) -> TypeExpr {
        TypeExpr::Parameterized {
            base: base.to_string(),
            params,
        }
    }

    #[test]
    fn test_bind_bare_typevar() {
        let mut inferred = HashMap::new();
        SharedCompileContext::bind_type_vars_from_expr(
            &tv("T"),
            &JuliaType::Int64,
            &["T"],
            &mut inferred,
        )
        .unwrap();
        assert_eq!(inferred.get("T"), Some(&JuliaType::Int64));
    }

    #[test]
    fn test_bind_nested_tuple_typevar() {
        // Tuple{T,T} vs Tuple{Int64,Int64} binds T = Int64
        let mut inferred = HashMap::new();
        let expr = param("Tuple", vec![tv("T"), tv("T")]);
        let actual = JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Int64]);
        SharedCompileContext::bind_type_vars_from_expr(&expr, &actual, &["T"], &mut inferred)
            .unwrap();
        assert_eq!(inferred.get("T"), Some(&JuliaType::Int64));
    }

    #[test]
    fn test_bind_nested_tuple_distinct_params() {
        // Tuple{S,T} vs Tuple{Int64,String} binds S=Int64, T=String
        let mut inferred = HashMap::new();
        let expr = param("Tuple", vec![tv("S"), tv("T")]);
        let actual = JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::String]);
        SharedCompileContext::bind_type_vars_from_expr(&expr, &actual, &["S", "T"], &mut inferred)
            .unwrap();
        assert_eq!(inferred.get("S"), Some(&JuliaType::Int64));
        assert_eq!(inferred.get("T"), Some(&JuliaType::String));
    }

    #[test]
    fn test_bind_vector_typevar() {
        // Vector{T} vs VectorOf(Int64) binds T = Int64
        let mut inferred = HashMap::new();
        let expr = param("Vector", vec![tv("T")]);
        let actual = JuliaType::VectorOf(Box::new(JuliaType::Int64));
        SharedCompileContext::bind_type_vars_from_expr(&expr, &actual, &["T"], &mut inferred)
            .unwrap();
        assert_eq!(inferred.get("T"), Some(&JuliaType::Int64));
    }

    #[test]
    fn test_bind_conflicting_numeric_does_not_unify() {
        // Tuple{T,T} vs Tuple{Int64,Float64}: a single `T` cannot be both
        // `Int64` and `Float64`. The default constructor `Foo(a::T, b::T)`
        // therefore has no matching method — upstream raises a `MethodError`
        // and does NOT widen `T` to `Float64` (Issue #8102). The binding is
        // reported as an error rather than silently unifying.
        let mut inferred = HashMap::new();
        let expr = param("Tuple", vec![tv("T"), tv("T")]);
        let actual = JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Float64]);
        let result =
            SharedCompileContext::bind_type_vars_from_expr(&expr, &actual, &["T"], &mut inferred);
        assert!(result.is_err());
    }

    #[test]
    fn test_bind_non_param_name_not_bound() {
        // A name not declared as a struct type param must not be bound.
        let mut inferred = HashMap::new();
        SharedCompileContext::bind_type_vars_from_expr(
            &tv("NotAParam"),
            &JuliaType::Int64,
            &["T"],
            &mut inferred,
        )
        .unwrap();
        assert!(inferred.is_empty());
    }

    #[test]
    fn test_infer_typevars_from_bounded_param_field_issue_8382() {
        let span = crate::span::Span::new(0, 0, 0, 0, 0, 0);
        let def = crate::ir::core::StructDef {
            name: "BI".to_string(),
            is_mutable: false,
            is_base_origin: false,
            type_params: vec![
                crate::types::TypeParam::new("Y".to_string()),
                crate::types::TypeParam::new("X".to_string()),
                crate::types::TypeParam::with_upper_bound(
                    "Ty".to_string(),
                    "AbstractVector{Y}".to_string(),
                ),
                crate::types::TypeParam::with_upper_bound(
                    "Tx".to_string(),
                    "AbstractVector{X}".to_string(),
                ),
                crate::types::TypeParam::new("F".to_string()),
            ],
            parent_type: None,
            fields: vec![
                crate::ir::core::StructField {
                    name: "f".to_string(),
                    type_expr: Some(tv("F")),
                    span,
                },
                crate::ir::core::StructField {
                    name: "y".to_string(),
                    type_expr: Some(tv("Ty")),
                    span,
                },
                crate::ir::core::StructField {
                    name: "x".to_string(),
                    type_expr: Some(tv("Tx")),
                    span,
                },
                crate::ir::core::StructField {
                    name: "n".to_string(),
                    type_expr: Some(TypeExpr::Concrete(JuliaType::Int64)),
                    span,
                },
            ],
            inner_constructors: vec![],
            span,
            global_new_helpers: Vec::new(),
        };
        let inferred = infer_parametric_type_args(
            &def,
            "BI",
            &[
                JuliaType::Any,
                JuliaType::VectorOf(Box::new(JuliaType::Float64)),
                JuliaType::VectorOf(Box::new(JuliaType::Float64)),
                JuliaType::Int64,
            ],
        )
        .unwrap();
        assert_eq!(
            inferred,
            vec![
                JuliaType::Float64,
                JuliaType::Float64,
                JuliaType::VectorOf(Box::new(JuliaType::Float64)),
                JuliaType::VectorOf(Box::new(JuliaType::Float64)),
                JuliaType::Any,
            ]
        );
    }
    #[test]
    fn test_parse_struct_type_params() {
        assert_eq!(
            parse_struct_type_params("Foo{Int64, String}", "Foo"),
            Some(vec![JuliaType::Int64, JuliaType::String])
        );
        // Nested braces respected.
        assert_eq!(
            parse_struct_type_params("Foo{Tuple{Int64, Int64}}", "Foo"),
            Some(vec![JuliaType::from_name_or_struct("Tuple{Int64, Int64}")])
        );
        // Base mismatch.
        assert_eq!(parse_struct_type_params("Bar{Int64}", "Foo"), None);
        // No params.
        assert_eq!(parse_struct_type_params("Foo", "Foo"), None);
    }
}
