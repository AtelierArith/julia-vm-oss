//! Shared compilation context for parametric type instantiation.
//!
//! This module manages struct definitions, parametric type instantiation,
//! and type information that is shared across all compiler instances.

use std::collections::{HashMap, HashSet};

use crate::ir::core::{Block, Function, MacroDef};
use crate::types::{JuliaType, TypeExpr, TypeParam};
use crate::vm::{AbstractTypeDefInfo, PrimitiveTypeDefInfo, StructDefInfo, ValueType};

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

/// Struct definition info for compilation.
#[derive(Debug, Clone)]
pub struct StructInfo {
    pub type_id: usize,
    pub is_mutable: bool,
    pub fields: Vec<(String, ValueType)>,
    /// True if this struct has inner constructors defined
    pub has_inner_constructor: bool,
}

/// Shared compilation context for parametric type instantiation.
/// This is shared across all compiler instances to track type instantiations.
pub struct SharedCompileContext {
    pub struct_table: HashMap<String, StructInfo>,
    pub struct_defs: Vec<StructDefInfo>,
    pub struct_name_to_def_index: HashMap<String, usize>,
    pub parametric_structs: HashMap<String, ParametricStructDef>,
    pub abstract_types: Vec<AbstractTypeDefInfo>,
    pub abstract_type_by_name: HashMap<String, usize>,
    pub type_id_to_struct_name: HashMap<usize, String>,
    pub instantiation_table: HashMap<InstantiationKey, usize>,
    pub next_type_id: usize,
    /// Top-level (global/const) variable types, available to all functions.
    pub global_types: HashMap<String, ValueType>,
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
    /// Function names that may gain methods through runtime `@eval`.
    pub runtime_eval_function_names: HashSet<String>,
    /// Global indices of methods introduced by runtime `@eval`.
    pub runtime_eval_function_indices: HashSet<usize>,
    /// Map from global function index to its IR for call-site type inference.
    pub function_ir_by_global_index: HashMap<usize, Function>,
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
    /// Closure captured variables: maps function name -> set of captured variable names.
    /// Used when compiling closures to know which variables to load via LoadCaptured.
    pub closure_captures: HashMap<String, std::collections::HashSet<String>>,
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
}

impl SharedCompileContext {
    pub fn new(
        struct_table: HashMap<String, StructInfo>,
        struct_defs: Vec<StructDefInfo>,
        parametric_structs: HashMap<String, ParametricStructDef>,
        abstract_types: Vec<AbstractTypeDefInfo>,
        next_type_id: usize,
    ) -> Self {
        Self::with_instantiation_table(
            struct_table,
            struct_defs,
            parametric_structs,
            abstract_types,
            next_type_id,
            HashMap::new(),
        )
    }

    /// Create with a pre-populated instantiation table (for caching).
    pub fn with_instantiation_table(
        struct_table: HashMap<String, StructInfo>,
        struct_defs: Vec<StructDefInfo>,
        parametric_structs: HashMap<String, ParametricStructDef>,
        abstract_types: Vec<AbstractTypeDefInfo>,
        next_type_id: usize,
        instantiation_table: HashMap<InstantiationKey, usize>,
    ) -> Self {
        let mut struct_name_to_def_index = HashMap::new();
        for (idx, def) in struct_defs.iter().enumerate() {
            struct_name_to_def_index.insert(def.name.clone(), idx);
        }

        let mut type_id_to_struct_name = HashMap::new();
        for (idx, def) in struct_defs.iter().enumerate() {
            let type_id = struct_table
                .get(&def.name)
                .map(|info| info.type_id)
                .unwrap_or(idx);
            type_id_to_struct_name
                .entry(type_id)
                .or_insert_with(|| def.name.clone());
        }
        for (name, info) in &struct_table {
            type_id_to_struct_name
                .entry(info.type_id)
                .or_insert_with(|| name.clone());
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
            abstract_types,
            abstract_type_by_name,
            type_id_to_struct_name,
            instantiation_table,
            next_type_id,
            global_types: HashMap::new(),
            global_const_structs: HashMap::new(),
            spec_func_mapping: HashMap::new(),
            macros: HashMap::new(),
            function_indices: HashMap::new(),
            runtime_eval_function_names: HashSet::new(),
            runtime_eval_function_indices: HashSet::new(),
            function_ir_by_global_index: HashMap::new(),
            type_aliases: HashMap::new(),
            module_imported_bindings: HashMap::new(),
            closure_captures: HashMap::new(),
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
        if struct_name == bound_name {
            return true;
        }

        // Find the struct in struct_defs
        let struct_def = self
            .struct_name_to_def_index
            .get(struct_name)
            .and_then(|idx| self.struct_defs.get(*idx));
        if let Some(def) = struct_def {
            if let Some(parent) = &def.parent_type {
                // Check if parent matches the bound
                if parent == bound_name {
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

        self.expanded_bound_type(bound_name)
            .is_some_and(|bound| jt.is_subtype_of(&bound))
    }

    pub(crate) fn type_name_satisfies_bound(&self, type_name: &str, bound_name: &str) -> bool {
        if self.check_struct_satisfies_bound(type_name, bound_name) {
            return true;
        }

        self.expanded_bound_type(bound_name)
            .is_some_and(|bound| JuliaType::from_name_or_struct(type_name).is_subtype_of(&bound))
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
        let type_name = type_name.trim();
        if type_name.is_empty() {
            return Some(type_name.to_string());
        }

        if !bound_alias_name_is_excluded(type_name, excluded) {
            if let Some(target) = self.resolve_bound_type_alias(type_name) {
                if !seen.insert(type_name.to_string()) {
                    return Some(type_name.to_string());
                }
                let expanded =
                    self.expand_type_aliases_in_type_name_excluding(&target, seen, excluded);
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
                    self.expand_type_aliases_in_type_name_excluding(
                        &arg.to_string(),
                        seen,
                        excluded,
                    )
                })
                .collect::<Option<Vec<_>>>()?;
            return Some(format!("Union{{{}}}", expanded_args.join(", ")));
        }

        if let Some((base, params)) = parse_parametric_call(type_name) {
            let expanded_base = self
                .expand_type_aliases_in_type_name_excluding(&base, seen, excluded)
                .unwrap_or(base);
            let expanded_params = params
                .iter()
                .map(|param| {
                    self.expand_type_aliases_in_type_name_excluding(
                        &param.to_string(),
                        seen,
                        excluded,
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
        // If they match, it satisfies
        if type_name == bound_name {
            return true;
        }

        // Find the abstract type in abstract_types
        if let Some(at) = self
            .abstract_type_by_name
            .get(type_name)
            .and_then(|idx| self.abstract_types.get(*idx))
        {
            if let Some(parent) = &at.parent {
                // Check if parent matches the bound
                if parent == bound_name {
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
        let key = InstantiationKey {
            base_name: base_name.to_string(),
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
        let parametric_def = self.parametric_structs.get(base_name).ok_or_else(|| {
            CompileError::Msg(format!("Unknown parametric struct: {}", base_name))
        })?;
        let def = parametric_def.def.clone();

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
            let vt = self.substitute_field_type(&f.type_expr, &type_subst)?;
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
        let instantiated_name = TypeExpr::format_parameterized(base_name, type_args);

        // All parametric structs (including Complex) get sequential type_ids
        let type_id = self.next_type_id;
        self.next_type_id += 1;

        // Register in struct_table
        self.struct_table.insert(
            instantiated_name.clone(),
            StructInfo {
                type_id,
                is_mutable: def.is_mutable,
                fields: fields.clone(),
                has_inner_constructor: !def.inner_constructors.is_empty(),
            },
        );

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

    /// Record a single type-variable binding for a parametric struct's *default*
    /// constructor. Every occurrence of the same type variable must resolve to
    /// the **identical** concrete type — there is **no numeric promotion**.
    ///
    /// The default constructor of `struct Foo{T}; a::T; b::T; end` is
    /// `Foo(a::T, b::T) where {T}`, which only matches when both arguments share
    /// one concrete `T`. `Foo(1, 2.0)` therefore has *no* matching default
    /// method (a single `T` cannot be both `Int64` and `Float64`) and upstream
    /// raises a `MethodError` — it must **not** widen to `Foo{Float64}` (Issue
    /// #8102). Promotion is only valid for the *explicit* `Foo{Float64}(1, 2.0)`
    /// form, which converts on a separate code path. Two distinct concrete
    /// bindings are reported as an error so the caller can surface the
    /// `MethodError`.
    ///
    /// `JuliaType::Any` is treated as an "unknown" placeholder rather than a
    /// concrete type: when a constructor argument's type cannot be pinned down
    /// at compile time it infers to `Any`, and that occurrence must *refine* to
    /// (or defer to) any concrete binding of the same variable instead of
    /// conflicting with it. This keeps imprecise-but-valid constructions — e.g.
    /// `Truncated(d, 0.0, 1.0, ...)` where some fields infer to `Any` — working,
    /// while still rejecting genuinely non-unifiable concrete pairs.
    fn record_binding(
        name: &str,
        actual: &JuliaType,
        inferred: &mut HashMap<String, JuliaType>,
    ) -> CResult<()> {
        if let Some(existing) = inferred.get(name) {
            if existing == actual {
                return Ok(());
            }
            // The bindings differ. Only an imprecise `Any` placeholder may be
            // reconciled; two distinct concrete types do not unify (no
            // promotion) — Issue #8102.
            if *existing != JuliaType::Any && *actual != JuliaType::Any {
                return err(format!(
                    "Inconsistent type inference for {}: {} vs {}",
                    name, existing, actual
                ));
            }
            // This occurrence is the unknown `Any`: keep the concrete binding.
            if *actual == JuliaType::Any {
                return Ok(());
            }
            // The prior binding was the unknown `Any`: refine to the concrete
            // type (fall through to the insert below).
        }
        inferred.insert(name.to_string(), actual.clone());
        Ok(())
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
    fn bind_type_vars_from_expr(
        type_expr: &TypeExpr,
        actual: &JuliaType,
        param_names: &[&str],
        inferred: &mut HashMap<String, JuliaType>,
    ) -> CResult<()> {
        match type_expr {
            TypeExpr::TypeVar(name) => {
                // Only bind names that are declared type parameters of the struct.
                if param_names.contains(&name.as_str()) {
                    Self::record_binding(name, actual, inferred)?;
                }
                Ok(())
            }
            TypeExpr::Concrete(_) => Ok(()),
            TypeExpr::RuntimeExpr(_) => Ok(()),
            TypeExpr::Parameterized { base, params } => {
                match (base.as_str(), actual) {
                    // Tuple{T1, T2, ...} vs TupleOf([...])
                    ("Tuple", JuliaType::TupleOf(elems)) if elems.len() == params.len() => {
                        for (p, e) in params.iter().zip(elems.iter()) {
                            Self::bind_type_vars_from_expr(p, e, param_names, inferred)?;
                        }
                    }
                    // Array{T}/Vector{T} vs VectorOf(elem)
                    ("Array" | "Vector", JuliaType::VectorOf(elem)) if !params.is_empty() => {
                        Self::bind_type_vars_from_expr(&params[0], elem, param_names, inferred)?;
                    }
                    // Array{T}/Matrix{T} vs MatrixOf(elem)
                    ("Array" | "Matrix", JuliaType::MatrixOf(elem)) if !params.is_empty() => {
                        Self::bind_type_vars_from_expr(&params[0], elem, param_names, inferred)?;
                    }
                    // Parametric struct Foo{T,...} vs Struct("Foo{Int64,...}")
                    (_, JuliaType::Struct(actual_name)) => {
                        if let Some(actual_params) = parse_struct_type_params(actual_name, base) {
                            if actual_params.len() == params.len() {
                                for (p, a) in params.iter().zip(actual_params.iter()) {
                                    Self::bind_type_vars_from_expr(p, a, param_names, inferred)?;
                                }
                            }
                        }
                    }
                    _ => {
                        // Shapes don't align (e.g. actual is Any): leave embedded
                        // type vars unbound. The caller reports unbound params.
                    }
                }
                Ok(())
            }
        }
    }

    fn bind_type_vars_from_param_bounds(
        type_params: &[crate::types::TypeParam],
        inferred: &mut HashMap<String, JuliaType>,
    ) -> CResult<()> {
        let mut changed = true;
        while changed {
            changed = false;
            for param in type_params {
                let Some(actual) = inferred.get(&param.name).cloned() else {
                    continue;
                };
                let Some(bound_name) = param.get_upper_bound() else {
                    continue;
                };
                let bound_pattern = JuliaType::from_name_or_struct(bound_name);
                let Some(bindings) = actual.extract_type_bindings(&bound_pattern, type_params)
                else {
                    continue;
                };
                for (name, ty) in bindings {
                    let previous = inferred.get(&name).cloned();
                    Self::record_binding(&name, &ty, inferred)?;
                    if previous.as_ref() != inferred.get(&name) {
                        changed = true;
                    }
                }
            }
        }
        Ok(())
    }

    /// Substitute type parameters in a field type and convert to ValueType.
    /// Handles nested parameterized types like Array{T} or Point{Float64}.
    pub fn substitute_field_type(
        &mut self,
        type_expr: &Option<TypeExpr>,
        type_subst: &HashMap<String, TypeExpr>,
    ) -> CResult<ValueType> {
        match type_expr {
            None => Ok(ValueType::Any), // Untyped fields are Any (Julia semantics)
            Some(TypeExpr::Concrete(jt)) => {
                // Handle JuliaType::Struct specially - look up type_id from struct_table
                match jt {
                    JuliaType::Struct(name) => {
                        if let Some(info) = self.struct_table.get(name) {
                            Ok(ValueType::Struct(info.type_id))
                        } else {
                            // Struct not yet defined, fallback to Any
                            Ok(ValueType::Any)
                        }
                    }
                    _ => Ok(julia_type_to_value_type(jt)),
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
                    self.substitute_field_type(&Some(substituted.clone()), type_subst)
                } else {
                    // Not in type_subst - check if it's a known struct or type name
                    if let Some(info) = self.struct_table.get(name) {
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

                // Check if this is a known parametric struct
                if self.parametric_structs.contains_key(base) {
                    let type_id =
                        self.resolve_instantiation_with_type_expr(base, &resolved_params)?;
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

    /// Get any type_id for a struct with the given base name
    pub fn get_struct_type_id(&self, base_name: &str) -> Option<usize> {
        // Check struct_table for exact base name
        if let Some(info) = self.struct_table.get(base_name) {
            return Some(info.type_id);
        }
        // Check instantiation_table for parametric instantiations
        for (key, type_id) in &self.instantiation_table {
            if key.base_name == base_name {
                return Some(*type_id);
            }
        }
        // Scan struct_defs
        for (idx, def) in self.struct_defs.iter().enumerate() {
            if self.is_struct_of_base(&def.name, base_name) {
                return Some(idx);
            }
        }
        None
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
    if arg_types.len() != def.fields.len() {
        return err(format!(
            "{} constructor expects {} arguments, got {}",
            base_name,
            def.fields.len(),
            arg_types.len()
        ));
    }

    let mut inferred: HashMap<String, JuliaType> = HashMap::new();

    // Names declared as type parameters of this struct. Only these should be
    // treated as bindable type variables inside field type expressions.
    let param_names: Vec<&str> = def.type_params.iter().map(|p| p.name.as_str()).collect();

    for (field, arg_ty) in def.fields.iter().zip(arg_types.iter()) {
        if let Some(type_expr) = &field.type_expr {
            SharedCompileContext::bind_type_vars_from_expr(
                type_expr,
                arg_ty,
                &param_names,
                &mut inferred,
            )?;
        }
    }
    SharedCompileContext::bind_type_vars_from_param_bounds(&def.type_params, &mut inferred)?;

    // Build result in the order of type_params
    let mut result = Vec::new();
    for param in &def.type_params {
        let ty = inferred.get(&param.name).cloned().ok_or_else(|| {
            CompileError::Msg(format!(
                "Cannot infer type parameter {} for {}",
                param.name, base_name
            ))
        })?;
        result.push(ty);
    }

    Ok(result)
}

/// Parse the type parameters of a parametric struct instantiation name.
///
/// Given an instantiated struct name like `"Foo{Int64, String}"` and the
/// expected base `"Foo"`, returns `Some(vec![Int64, String])`. Returns `None`
/// if the name's base does not match `base`, or if the name carries no
/// parameters.
fn parse_struct_type_params(name: &str, base: &str) -> Option<Vec<JuliaType>> {
    let brace_idx = name.find('{')?;
    if &name[..brace_idx] != base {
        return None;
    }
    let close = name.rfind('}')?;
    if close <= brace_idx + 1 {
        return None;
    }
    let inner = &name[brace_idx + 1..close];
    let mut params = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, c) in inner.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                params.push(JuliaType::from_name_or_struct(inner[start..i].trim()));
                start = i + 1;
            }
            _ => {}
        }
    }
    let last = inner[start..].trim();
    if !last.is_empty() {
        params.push(JuliaType::from_name_or_struct(last));
    }
    if params.is_empty() {
        None
    } else {
        Some(params)
    }
}

fn type_param_base_name(name: &str) -> &str {
    name.split_once("<:")
        .or_else(|| name.split_once(">:"))
        .map_or(name, |(base, _)| base)
        .trim()
}

fn bound_alias_name_is_excluded(type_name: &str, excluded: &HashSet<String>) -> bool {
    let leaf = type_name.rsplit('.').next().unwrap_or(type_name);
    excluded.contains(type_name) || excluded.contains(leaf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::{AbstractTypeDefInfo, StructDefInfo, ValueType};

    /// Helper: build a minimal SharedCompileContext with the given structs and abstract types.
    fn make_ctx(
        structs: Vec<(&str, usize, Option<&str>)>, // (name, type_id, parent)
        abstract_types: Vec<(&str, Option<&str>)>, // (name, parent)
    ) -> SharedCompileContext {
        let mut struct_table = HashMap::new();
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
