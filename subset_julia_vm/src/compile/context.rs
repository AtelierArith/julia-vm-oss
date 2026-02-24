//! Shared compilation context for parametric type instantiation.
//!
//! This module manages struct definitions, parametric type instantiation,
//! and type information that is shared across all compiler instances.

use std::collections::HashMap;

use crate::ir::core::{Block, Function, MacroDef};
use crate::types::{JuliaType, TypeExpr};
use crate::vm::{AbstractTypeDefInfo, StructDefInfo, ValueType};

use super::types::{err, CResult, CompileError, InstantiationKey, ParametricStructDef};
use super::{
    check_type_satisfies_bound, julia_type_to_value_type, type_expr_to_string, widen_numeric_types,
};

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
    /// Map from global function index to its IR for call-site type inference.
    pub function_ir_by_global_index: HashMap<usize, Function>,
    /// Type aliases: maps alias name -> target type name
    /// For `const MyInt = Int64`, stores ("MyInt" -> "Int64")
    pub type_aliases: HashMap<String, String>,
    /// Closure captured variables: maps function name -> set of captured variable names.
    /// Used when compiling closures to know which variables to load via LoadCaptured.
    pub closure_captures: HashMap<String, std::collections::HashSet<String>>,
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
            function_ir_by_global_index: HashMap::new(),
            type_aliases: HashMap::new(),
            closure_captures: HashMap::new(),
        }
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
                        type_args
                            .iter()
                            .map(type_expr_to_string)
                            .collect::<Vec<_>>()
                            .join(", "),
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
                        if !check_type_satisfies_bound(jt, bound_name) {
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
                        if !self.check_struct_satisfies_bound(type_name, bound_name) {
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
        for f in &def.fields {
            let vt = self.substitute_field_type(&f.type_expr, &type_subst)?;
            fields.push((f.name.clone(), vt));
        }

        // Build instantiated name for display (e.g., "Point{Float64}" or "Container{Point{Float64}}")
        let type_args_str: Vec<String> = type_args.iter().map(type_expr_to_string).collect();
        let instantiated_name = format!("{}{{{}}}", base_name, type_args_str.join(", "));

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
        let def = &parametric_def.def;

        if arg_types.len() != def.fields.len() {
            return err(format!(
                "{} constructor expects {} arguments, got {}",
                base_name,
                def.fields.len(),
                arg_types.len()
            ));
        }

        let mut inferred: HashMap<String, JuliaType> = HashMap::new();

        for (field, arg_ty) in def.fields.iter().zip(arg_types.iter()) {
            if let Some(TypeExpr::TypeVar(name)) = &field.type_expr {
                if let Some(existing) = inferred.get(name) {
                    if existing != arg_ty {
                        // Try to widen numeric types (e.g., Int64 + Float64 -> Float64)
                        if let Some(widened) = widen_numeric_types(existing, arg_ty) {
                            inferred.insert(name.clone(), widened);
                        } else {
                            return err(format!(
                                "Inconsistent type inference for {}: {} vs {}",
                                name, existing, arg_ty
                            ));
                        }
                    }
                } else {
                    inferred.insert(name.clone(), arg_ty.clone());
                }
            }
        }

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
        SharedCompileContext::new(struct_table, struct_defs, HashMap::new(), abs_types, structs.len())
    }

    // ── check_struct_satisfies_bound ─────────────────────────────────────────

    #[test]
    fn test_bound_any_always_satisfied() {
        let ctx = make_ctx(vec![("Dog", 0, Some("Animal"))], vec![("Animal", None)]);
        assert!(ctx.check_struct_satisfies_bound("Dog", "Any"));
        assert!(ctx.check_struct_satisfies_bound("UnknownType", "Any"));
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
            vec![("Dog", Some("Mammal")), ("Mammal", Some("Animal")), ("Animal", None)],
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
}

