//! Method table for multiple dispatch support.
//!
//! Contains MethodSig and MethodTable structures for tracking function
//! methods with type information and performing dispatch.

// SAFETY: i32→u32 cast at score computation is guarded by `.max(0)` before the cast.
#![allow(clippy::cast_sign_loss)]

use std::cell::RefCell;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// =============================================================================
// Dispatch Scoring Constants (Issue #2295)
// =============================================================================
//
// These constants control the match quality bonuses/penalties used in method
// dispatch scoring. They must maintain the following invariant:
//
// **Invariant**: When an argument has compile-time type `Any`, methods with
// `Any` parameters should be preferred over methods with specific parameters.
// This is because we don't have enough type information to confidently select
// the specific method.
//
// The penalty for Any-arg matching specific-param should fully negate the bonus
// for exact concrete primitive type matches, ensuring that:
//   - `f(::Any)` is preferred over `f(::Int64)` when called with unknown type
//   - But `f(::Int64)` is preferred over `f(::Any)` when called with `Int64`

/// Bonus for exact match between two concrete primitive types (e.g., Int64 == Int64).
/// This ensures f(::Bool) beats f(::Int64) when called with Bool.
const EXACT_PRIMITIVE_MATCH_BONUS: i32 = 10;

/// Penalty when argument type is `Any` but parameter type is specific.
/// This ensures methods with `Any` parameters are preferred when argument type is unknown.
/// Must equal `-EXACT_PRIMITIVE_MATCH_BONUS` to fully negate the bonus.
const ANY_ARG_SPECIFIC_PARAM_PENALTY: i32 = -10;

use crate::types::{DispatchError, JuliaType, TypeParam};
use crate::vm::ValueType;

/// A method signature with type information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MethodSig {
    /// Index into the methods list for this function name.
    pub _method_index: usize,
    /// Global function index (for bytecode).
    pub global_index: usize,
    /// Parameter names and their declared types.
    pub params: Vec<(String, JuliaType)>,
    /// Inferred return type.
    pub return_type: ValueType,
    /// Parametric return type that preserves element-level type info (Issue #2317).
    /// `ValueType::Tuple` loses element types; this field carries `JuliaType::TupleOf(...)`
    /// when the abstract interpretation engine infers a parametric tuple return type.
    pub return_julia_type: Option<JuliaType>,
    /// True if this method extends a Base operator (e.g., `function Base.:+(...)`)
    /// Base extension methods do NOT shadow builtin operators for primitive types.
    pub is_base_extension: bool,
    /// Type parameters from where clause (e.g., `where T<:Real`).
    /// Used for parametric dispatch matching.
    pub type_params: Vec<TypeParam>,
    /// Index of varargs parameter (if any). For `f(a, args...)`, this would be Some(1).
    pub vararg_param_index: Option<usize>,
    /// For Vararg{T, N}: fixed argument count N. None = any count. (Issue #2525)
    pub vararg_fixed_count: Option<usize>,
}

/// Method table for a function name (supports multiple dispatch).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MethodTable {
    pub name: String,
    pub methods: Vec<MethodSig>,
    /// Map from concrete struct name to its declared parent abstract type name.
    /// Populated by the compiler from struct definitions; used to break ambiguous
    /// dispatch when both arg and param types are user-defined (Issue #3144).
    ///
    /// Example: `struct Car <: MotorVehicle` → `"Car" -> Some("MotorVehicle")`
    #[serde(default)]
    pub struct_parents: HashMap<String, Option<String>>,
    /// Dispatch result cache: maps argument types to the index of the best matching
    /// method in `self.methods`. Invalidated on `add_method()`. (Issue #3361)
    #[serde(skip)]
    dispatch_cache: RefCell<HashMap<Vec<JuliaType>, usize>>,
}

impl MethodTable {
    pub fn new(name: String) -> Self {
        Self {
            name,
            methods: Vec::new(),
            struct_parents: HashMap::new(),
            dispatch_cache: RefCell::new(HashMap::new()),
        }
    }

    /// Add a method to this table.
    /// If a method with the same signature already exists, replace it instead of adding a duplicate.
    pub fn add_method(&mut self, sig: MethodSig) {
        // Check if a method with the same parameter types already exists
        let params_match = |existing: &MethodSig| {
            existing.params.len() == sig.params.len()
                && existing
                    .params
                    .iter()
                    .zip(&sig.params)
                    .all(|(a, b)| a.1 == b.1)
        };

        // Find and replace existing method, or add new one
        if let Some(pos) = self.methods.iter().position(params_match) {
            // Replace existing method (user code overrides Base)
            self.methods[pos] = sig;
        } else {
            // Add new method
            self.methods.push(sig);
        }

        // Invalidate dispatch cache (Issue #3361)
        self.dispatch_cache.borrow_mut().clear();
    }

    /// Check if all methods in this table are Base extensions.
    /// If true, we should prefer builtin operators for primitive types.
    pub fn all_base_extensions(&self) -> bool {
        !self.methods.is_empty() && self.methods.iter().all(|m| m.is_base_extension)
    }

    /// Find the best matching method for the given argument types.
    pub fn dispatch(&self, arg_types: &[JuliaType]) -> Result<&MethodSig, DispatchError> {
        // Check dispatch cache first (Issue #3361)
        if let Some(&cached_idx) = self.dispatch_cache.borrow().get(arg_types) {
            if cached_idx < self.methods.len() {
                return Ok(&self.methods[cached_idx]);
            }
        }

        let result = self.dispatch_inner(arg_types)?;

        // Cache the result by finding its index via pointer comparison
        let result_ptr = result as *const MethodSig;
        for (i, m) in self.methods.iter().enumerate() {
            if std::ptr::eq(m as *const MethodSig, result_ptr) {
                self.dispatch_cache
                    .borrow_mut()
                    .insert(arg_types.to_vec(), i);
                break;
            }
        }

        Ok(result)
    }

    /// Inner dispatch logic (uncached). (Issue #3361)
    fn dispatch_inner(&self, arg_types: &[JuliaType]) -> Result<&MethodSig, DispatchError> {
        let mut matches: Vec<(&MethodSig, u32)> = Vec::new();

        for method in &self.methods {
            // Check arity compatibility
            let arity_match = if let Some(vararg_idx) = method.vararg_param_index {
                if let Some(fixed_count) = method.vararg_fixed_count {
                    // Vararg{T, N}: exactly vararg_idx fixed params + N varargs (Issue #2525)
                    arg_types.len() == vararg_idx + fixed_count
                } else {
                    // Varargs: need at least (vararg_idx) fixed args, the rest go to varargs
                    arg_types.len() >= vararg_idx
                }
            } else {
                // No varargs: exact match required
                method.params.len() == arg_types.len()
            };

            if !arity_match {
                continue;
            }

            // Check if all args are subtypes of params (with parametric context)
            // For varargs, only check the fixed params
            let fixed_param_count = method.vararg_param_index.unwrap_or(method.params.len());

            // Track type variable bindings to ensure the same TypeVar binds to the same type
            // This is needed for methods like f(::Type{T}, ::Type{T}) where T - both args must be same type
            let match_result = check_method_match_with_bindings(
                &method
                    .params
                    .iter()
                    .take(fixed_param_count)
                    .map(|(_, ty)| ty.clone())
                    .collect::<Vec<_>>(),
                &arg_types
                    .iter()
                    .take(fixed_param_count)
                    .cloned()
                    .collect::<Vec<_>>(),
                &method.type_params,
            );

            if let Some(binding_count) = match_result {
                // Calculate specificity score
                // For varargs, give a small penalty so non-varargs methods are preferred
                //
                // The score has two components:
                // 1. Base specificity: sum of parameter type specificities
                // 2. Match quality bonus: extra points for exact CONCRETE PRIMITIVE type matches
                //
                // This ensures that when calling f(true) with overloads f(::Bool) and f(::Int64),
                // f(::Bool) wins because Bool matches Bool exactly, while Bool only matches Int64
                // via the subtype relationship Bool <: Integer.
                //
                // The key insight is that for concrete primitive types (like Bool and Int64),
                // an exact match should be preferred over a subtype match. But for struct types
                // and parametric types, we still want normal specificity-based dispatch.
                let base_score: u32 = method
                    .params
                    .iter()
                    .take(fixed_param_count)
                    .map(|(_, ty)| ty.specificity() as u32)
                    .sum();

                // Calculate match quality bonus/penalty for type matches.
                // - Gives bonus when BOTH the parameter type AND argument type are
                //   concrete primitive types (Bool, Int64, Float64, etc.) and they match exactly.
                // - Gives penalty when argument type is Any but parameter is specific.
                //   This avoids breaking dispatch for struct types like Rational.
                let match_quality_bonus: i32 = method
                    .params
                    .iter()
                    .take(fixed_param_count)
                    .zip(arg_types.iter().take(fixed_param_count))
                    .map(|((_, param_ty), arg_ty)| {
                        // Only give bonus for exact match of concrete primitive types
                        // This handles Bool vs Int64 dispatch correctly without affecting
                        // struct-based dispatch like Rational
                        if param_ty.is_concrete_primitive() && arg_ty.is_concrete_primitive() {
                            if param_ty == arg_ty {
                                EXACT_PRIMITIVE_MATCH_BONUS
                            } else {
                                0
                            }
                        } else if let (JuliaType::Struct(param_name), JuliaType::Struct(arg_name)) =
                            (param_ty, arg_ty)
                        {
                            // Exact parametric struct match bonus:
                            // Complex{Int64} param matching Complex{Int64} arg should win over
                            // bare Complex param matching Complex{Int64} arg.
                            if param_name == arg_name {
                                EXACT_PRIMITIVE_MATCH_BONUS
                            } else {
                                0
                            }
                        } else if matches!(arg_ty, JuliaType::Any)
                            && !matches!(param_ty, JuliaType::Any)
                        {
                            // Issue #1665: When arg type is Any (compile-time unknown),
                            // prefer methods with Any parameters over specific types.
                            // This ensures map(f::Function, A) is selected over map(f::Function, x::Int64)
                            // when the second argument has unknown type.
                            ANY_ARG_SPECIFIC_PARAM_PENALTY
                        } else {
                            0
                        }
                    })
                    .sum();

                let score = (base_score as i32 + match_quality_bonus).max(0) as u32;

                // Add bonus for methods that reuse type variables (more constrained)
                // E.g., `f(::Type{T}, ::Type{T}) where T` is more specific than `f(::Type{T}, ::Type{S}) where {T, S}`
                // because it requires both arguments to be the same type.
                // When binding_count < fixed_param_count, it means type variables were reused.
                let type_reuse_bonus = if binding_count < fixed_param_count {
                    // Bonus proportional to how many parameters reuse existing bindings
                    (fixed_param_count - binding_count) as u32
                } else {
                    0
                };

                // Varargs methods get lower score (less specific)
                // But fixed-count varargs (Vararg{T,N}) are more specific than unconstrained (Issue #2525)
                let adjusted_score = if method.vararg_param_index.is_some() {
                    if method.vararg_fixed_count.is_some() {
                        // Fixed-count vararg is almost as specific as non-vararg
                        score + type_reuse_bonus
                    } else {
                        score.saturating_sub(1) + type_reuse_bonus
                    }
                } else {
                    score + type_reuse_bonus
                };
                matches.push((method, adjusted_score));
            }
        }

        if matches.is_empty() {
            return Err(DispatchError::NoMethodFound {
                name: self.name.clone(),
                arg_types: arg_types.to_vec(),
            });
        }

        // Find max specificity
        let max_score = matches.iter().map(|(_, s)| *s).max().unwrap();
        let best: Vec<_> = matches.iter().filter(|(_, s)| *s == max_score).collect();

        if best.len() > 1 {
            // Combined tie-breaker pass: compute all criteria in one loop (Issue #3361)
            let has_any_arg = arg_types.iter().any(|t| matches!(t, JuliaType::Any));
            let mut max_any_count = 0usize;
            let mut non_varargs: Vec<&(&MethodSig, u32)> = Vec::new();
            let mut ancestry_passed: Vec<&(&MethodSig, u32)> = Vec::new();
            let mut any_counts: Vec<(&(&MethodSig, u32), usize)> = Vec::new();

            for entry in &best {
                let m = entry.0;
                let fixed_count = m.vararg_param_index.unwrap_or(m.params.len());

                // Tie-breaker 1 data: count Any params
                if has_any_arg {
                    let any_count = m
                        .params
                        .iter()
                        .take(fixed_count)
                        .filter(|(_, ty)| matches!(ty, JuliaType::Any))
                        .count();
                    if any_count > max_any_count {
                        max_any_count = any_count;
                    }
                    any_counts.push((entry, any_count));
                }

                // Tie-breaker 2 data: non-varargs
                if m.vararg_param_index.is_none() {
                    non_varargs.push(entry);
                }

                // Tie-breaker 3 data: struct ancestry filter
                if !self.struct_parents.is_empty() {
                    let passes = m
                        .params
                        .iter()
                        .take(fixed_count)
                        .zip(arg_types.iter().take(fixed_count))
                        .all(|((_, param_ty), arg_ty)| {
                            if let (
                                JuliaType::AbstractUser(abstract_name, _),
                                JuliaType::Struct(struct_name),
                            ) = (param_ty, arg_ty)
                            {
                                struct_is_subtype_of_abstract(
                                    struct_name,
                                    abstract_name,
                                    &self.struct_parents,
                                )
                            } else {
                                true
                            }
                        });
                    if passes {
                        ancestry_passed.push(entry);
                    }
                }
            }

            // Apply tie-breaker 1: prefer methods with most Any params when arg is Any
            if has_any_arg {
                let most_any: Vec<_> = any_counts
                    .iter()
                    .filter(|(_, c)| *c == max_any_count)
                    .map(|(entry, _)| *entry)
                    .collect();
                if most_any.len() == 1 {
                    return Ok(most_any[0].0);
                }
            }

            // Apply tie-breaker 2: prefer non-varargs
            if non_varargs.len() == 1 {
                return Ok(non_varargs[0].0);
            }

            // Apply tie-breaker 3: struct ancestry filter (Issue #3144)
            if !self.struct_parents.is_empty() && ancestry_passed.len() == 1 {
                return Ok(ancestry_passed[0].0);
            }

            return Err(DispatchError::AmbiguousMethod {
                name: self.name.clone(),
                arg_types: arg_types.to_vec(),
                candidates: best
                    .iter()
                    .map(|(m, _)| m.params.iter().map(|(_, ty)| ty.clone()).collect())
                    .collect(),
            });
        }

        Ok(best[0].0)
    }
}

/// Check if argument types match parameter types while tracking type variable bindings.
///
/// When a type variable (like T in `f(::Type{T}, ::Type{T}) where T`) appears multiple times,
/// it must bind to the same concrete type. For example:
/// - `f(Int64, Int64)` matches because T=Int64 for both
/// - `f(Int64, Float64)` does NOT match because T can't be both Int64 and Float64
///
/// Returns:
/// - None if the method doesn't match
/// - Some(n) if it matches, where n is the number of unique type variable bindings created
fn check_method_match_with_bindings(
    param_types: &[JuliaType],
    arg_types: &[JuliaType],
    type_params: &[TypeParam],
) -> Option<usize> {
    // Track bindings: type variable name -> concrete type it's bound to
    let mut bindings: HashMap<String, JuliaType> = HashMap::new();

    for (param_ty, arg_ty) in param_types.iter().zip(arg_types.iter()) {
        if !check_single_match_with_bindings(param_ty, arg_ty, type_params, &mut bindings) {
            return None;
        }
    }

    // Diagonal Rule (Issue #2554): type variables that appear 2+ times in covariant
    // position and 0 times in invariant position must be bound to concrete types.
    if !bindings.is_empty() && !JuliaType::check_diagonal_rule_for_params(param_types, &bindings) {
        return None;
    }

    Some(bindings.len())
}

/// Check if a single argument type matches a parameter type, updating bindings.
fn check_single_match_with_bindings(
    param_ty: &JuliaType,
    arg_ty: &JuliaType,
    type_params: &[TypeParam],
    bindings: &mut HashMap<String, JuliaType>,
) -> bool {
    // Handle TypeOf patterns (Type{T} parameters)
    if let JuliaType::TypeOf(inner_param) = param_ty {
        if let JuliaType::TypeOf(inner_arg) = arg_ty {
            // Both are Type{X} patterns
            // Check if inner_param is a type variable that needs binding
            if let JuliaType::TypeVar(var_name, bound) = inner_param.as_ref() {
                // Check if this type variable is from the method's where clause
                if type_params.iter().any(|tp| &tp.name == var_name) {
                    // It's a type variable that needs binding
                    let concrete_type = inner_arg.as_ref().clone();

                    // Check bound constraint if present
                    if let Some(bound_name) = bound {
                        if let Some(bound_type) = JuliaType::from_name(bound_name) {
                            if !concrete_type.is_subtype_of(&bound_type) {
                                return false;
                            }
                        }
                    }

                    // Check if we already have a binding for this variable
                    if let Some(existing) = bindings.get(var_name) {
                        // The same type variable must bind to the same type
                        if existing != &concrete_type {
                            return false;
                        }
                    } else {
                        // First occurrence - create binding
                        bindings.insert(var_name.clone(), concrete_type);
                    }
                    return true;
                }
            }
            // If inner_param is a concrete type, check for exact match
            return inner_arg.is_subtype_of(inner_param);
        }
    }

    // For non-TypeOf patterns, fall back to standard subtype check
    arg_ty.is_subtype_of_parametric(param_ty, type_params)
}

/// Check whether a concrete struct is a subtype of an abstract type using the
/// struct parent map (Issue #3144).
///
/// Walks up the struct's declared parent chain until the abstract type is found
/// (returns true) or the chain ends without a match (returns false).
///
/// If `struct_name` is not in the map (unknown struct), returns `true` to preserve
/// the previous conservative behaviour.
fn struct_is_subtype_of_abstract(
    struct_name: &str,
    abstract_name: &str,
    struct_parents: &HashMap<String, Option<String>>,
) -> bool {
    // If struct_name == abstract_name (parametric base name), accept
    // e.g., "Rational" matches abstract "Rational" (unusual, but safe)
    if struct_name == abstract_name {
        return true;
    }

    // If the struct is unknown, conservatively accept (old behaviour)
    let Some(parent_opt) = struct_parents.get(struct_name) else {
        return true;
    };

    // Walk the parent chain
    let mut current: Option<String> = parent_opt.clone();
    // Guard against cycles (shouldn't exist in valid Julia code)
    let mut visited = 0usize;
    while let Some(parent) = current {
        visited += 1;
        if visited > 32 {
            // Cycle guard: give up and conservatively accept
            return true;
        }
        // Strip type parameters from parent name for comparison
        let parent_base = parent.split('{').next().unwrap_or(&parent);
        if parent_base == abstract_name {
            return true;
        }
        // Walk up: look for the parent in the struct_parents map
        current = match struct_parents.get(parent_base) {
            Some(grandparent_opt) => grandparent_opt.clone(),
            None => break,
        };
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that the scoring constants maintain their invariant relationship.
    /// The penalty for Any-arg matching specific-param must fully negate the
    /// bonus for exact primitive match to ensure methods with Any parameters
    /// are preferred when argument type is unknown.
    #[test]
    fn test_scoring_constants_invariant() {
        // The invariant: penalty should fully negate bonus
        assert_eq!(
            EXACT_PRIMITIVE_MATCH_BONUS + ANY_ARG_SPECIFIC_PARAM_PENALTY,
            0,
            "ANY_ARG_SPECIFIC_PARAM_PENALTY must equal -EXACT_PRIMITIVE_MATCH_BONUS"
        );
    }

    /// Test that when argument type is Any, methods with Any parameters are
    /// preferred over methods with specific parameters (Issue #1665).
    #[test]
    fn test_any_arg_prefers_any_param_method() {
        let mut table = MethodTable::new("f".to_string());

        // Method 1: f(::Any)
        table.add_method(MethodSig {
            _method_index: 0,
            global_index: 0,
            params: vec![("x".to_string(), JuliaType::Any)],
            return_type: ValueType::Any,
            return_julia_type: None,
            is_base_extension: false,
            type_params: vec![],
            vararg_param_index: None,
            vararg_fixed_count: None,
        });

        // Method 2: f(::Int64) - more specific
        table.add_method(MethodSig {
            _method_index: 1,
            global_index: 1,
            params: vec![("x".to_string(), JuliaType::Int64)],
            return_type: ValueType::I64,
            return_julia_type: None,
            is_base_extension: false,
            type_params: vec![],
            vararg_param_index: None,
            vararg_fixed_count: None,
        });

        // When called with Any, should prefer f(::Any)
        let result = table.dispatch(&[JuliaType::Any]);
        assert!(result.is_ok(), "Dispatch should succeed");
        let method = result.unwrap();
        assert_eq!(
            method.params[0].1,
            JuliaType::Any,
            "Should select f(::Any) when argument type is Any"
        );
    }

    /// Test that when argument type is Int64, methods with Int64 parameters
    /// are still preferred over methods with Any parameters.
    #[test]
    fn test_concrete_arg_prefers_concrete_param_method() {
        let mut table = MethodTable::new("f".to_string());

        // Method 1: f(::Any)
        table.add_method(MethodSig {
            _method_index: 0,
            global_index: 0,
            params: vec![("x".to_string(), JuliaType::Any)],
            return_type: ValueType::Any,
            return_julia_type: None,
            is_base_extension: false,
            type_params: vec![],
            vararg_param_index: None,
            vararg_fixed_count: None,
        });

        // Method 2: f(::Int64)
        table.add_method(MethodSig {
            _method_index: 1,
            global_index: 1,
            params: vec![("x".to_string(), JuliaType::Int64)],
            return_type: ValueType::I64,
            return_julia_type: None,
            is_base_extension: false,
            type_params: vec![],
            vararg_param_index: None,
            vararg_fixed_count: None,
        });

        // When called with Int64, should prefer f(::Int64)
        let result = table.dispatch(&[JuliaType::Int64]);
        assert!(result.is_ok(), "Dispatch should succeed");
        let method = result.unwrap();
        assert_eq!(
            method.params[0].1,
            JuliaType::Int64,
            "Should select f(::Int64) when argument type is Int64"
        );
    }

    /// Test that the issue #1665 scenario works correctly:
    /// map(f::Function, A) should be preferred over map(f::Function, x::Int64)
    /// when the second argument has unknown type.
    #[test]
    fn test_issue_1665_map_dispatch_with_unknown_type() {
        let mut table = MethodTable::new("map".to_string());

        // Method 1: map(f::Function, A) - generic version
        table.add_method(MethodSig {
            _method_index: 0,
            global_index: 0,
            params: vec![
                ("f".to_string(), JuliaType::Function),
                ("A".to_string(), JuliaType::Any),
            ],
            return_type: ValueType::Any,
            return_julia_type: None,
            is_base_extension: false,
            type_params: vec![],
            vararg_param_index: None,
            vararg_fixed_count: None,
        });

        // Method 2: map(f::Function, x::Int64) - scalar version
        table.add_method(MethodSig {
            _method_index: 1,
            global_index: 1,
            params: vec![
                ("f".to_string(), JuliaType::Function),
                ("x".to_string(), JuliaType::Int64),
            ],
            return_type: ValueType::I64,
            return_julia_type: None,
            is_base_extension: false,
            type_params: vec![],
            vararg_param_index: None,
            vararg_fixed_count: None,
        });

        // When called with (Function, Any), should prefer map(f::Function, A)
        let result = table.dispatch(&[JuliaType::Function, JuliaType::Any]);
        assert!(result.is_ok(), "Dispatch should succeed");
        let method = result.unwrap();
        assert_eq!(
            method.params[1].1,
            JuliaType::Any,
            "Should select map(f::Function, A) when second argument type is Any (Issue #1665)"
        );
    }

    /// Test that dispatch correctly resolves sibling abstract type methods
    /// using struct parent information (Issue #3144).
    ///
    /// Scenario:
    ///   abstract type Vehicle end
    ///   abstract type MotorVehicle <: Vehicle end
    ///   abstract type NonMotorVehicle <: Vehicle end
    ///   struct Car <: MotorVehicle ...
    ///
    ///   vehicle_type(::MotorVehicle) and vehicle_type(::NonMotorVehicle) are both registered.
    ///   Calling vehicle_type(Car) should select the MotorVehicle method, NOT report ambiguity.
    #[test]
    fn test_abstract_sibling_dispatch_uses_struct_parents_issue_3144() {
        let mut table = MethodTable::new("vehicle_type".to_string());

        // Method 1: vehicle_type(::MotorVehicle) — global_index 10
        table.add_method(MethodSig {
            _method_index: 0,
            global_index: 10,
            params: vec![(
                "v".to_string(),
                JuliaType::AbstractUser("MotorVehicle".to_string(), Some("Vehicle".to_string())),
            )],
            return_type: ValueType::Any,
            return_julia_type: None,
            is_base_extension: false,
            type_params: vec![],
            vararg_param_index: None,
            vararg_fixed_count: None,
        });

        // Method 2: vehicle_type(::NonMotorVehicle) — global_index 11
        table.add_method(MethodSig {
            _method_index: 1,
            global_index: 11,
            params: vec![(
                "v".to_string(),
                JuliaType::AbstractUser(
                    "NonMotorVehicle".to_string(),
                    Some("Vehicle".to_string()),
                ),
            )],
            return_type: ValueType::Any,
            return_julia_type: None,
            is_base_extension: false,
            type_params: vec![],
            vararg_param_index: None,
            vararg_fixed_count: None,
        });

        // Without struct parent info: should be AmbiguousMethod (both match via old rule)
        let ambiguous_result = table.dispatch(&[JuliaType::Struct("Car".to_string())]);
        assert!(
            matches!(ambiguous_result, Err(crate::types::DispatchError::AmbiguousMethod { .. })),
            "Without struct_parents, dispatch should be ambiguous"
        );

        // Add struct parent info: Car <: MotorVehicle, Bicycle <: NonMotorVehicle
        table.struct_parents.insert("Car".to_string(), Some("MotorVehicle".to_string()));
        table
            .struct_parents
            .insert("Bicycle".to_string(), Some("NonMotorVehicle".to_string()));
        // Also need abstract types in the map (they appear in the parent chain)
        table
            .struct_parents
            .insert("MotorVehicle".to_string(), Some("Vehicle".to_string()));
        table
            .struct_parents
            .insert("NonMotorVehicle".to_string(), Some("Vehicle".to_string()));

        // With struct parent info: Car -> dispatch to MotorVehicle method (global_index 10)
        let car_result = table.dispatch(&[JuliaType::Struct("Car".to_string())]);
        assert!(
            car_result.is_ok(),
            "With struct_parents, dispatch of Car should succeed, got: {:?}",
            car_result
        );
        assert_eq!(
            car_result.unwrap().global_index,
            10,
            "Car should dispatch to MotorVehicle method (global_index 10)"
        );

        // Bicycle -> dispatch to NonMotorVehicle method (global_index 11)
        let bike_result = table.dispatch(&[JuliaType::Struct("Bicycle".to_string())]);
        assert!(
            bike_result.is_ok(),
            "Dispatch of Bicycle should succeed, got: {:?}",
            bike_result
        );
        assert_eq!(
            bike_result.unwrap().global_index,
            11,
            "Bicycle should dispatch to NonMotorVehicle method (global_index 11)"
        );
    }

    /// Test that dispatch cache returns the same result on second call (Issue #3361).
    #[test]
    fn test_dispatch_cache_hit() {
        let mut table = MethodTable::new("g".to_string());
        table.add_method(MethodSig {
            _method_index: 0,
            global_index: 0,
            params: vec![("x".to_string(), JuliaType::Int64)],
            return_type: ValueType::I64,
            return_julia_type: None,
            is_base_extension: false,
            type_params: vec![],
            vararg_param_index: None,
            vararg_fixed_count: None,
        });
        table.add_method(MethodSig {
            _method_index: 1,
            global_index: 1,
            params: vec![("x".to_string(), JuliaType::Any)],
            return_type: ValueType::Any,
            return_julia_type: None,
            is_base_extension: false,
            type_params: vec![],
            vararg_param_index: None,
            vararg_fixed_count: None,
        });

        // First call populates cache
        let r1 = table.dispatch(&[JuliaType::Int64]);
        assert!(r1.is_ok());
        assert_eq!(r1.unwrap().global_index, 0);

        // Second call should hit cache and return the same result
        let r2 = table.dispatch(&[JuliaType::Int64]);
        assert!(r2.is_ok());
        assert_eq!(r2.unwrap().global_index, 0);

        // Verify cache is populated
        assert_eq!(table.dispatch_cache.borrow().len(), 1);
    }

    /// Test that dispatch cache is invalidated when a method is added (Issue #3361).
    #[test]
    fn test_dispatch_cache_invalidation() {
        let mut table = MethodTable::new("h".to_string());
        table.add_method(MethodSig {
            _method_index: 0,
            global_index: 0,
            params: vec![("x".to_string(), JuliaType::Any)],
            return_type: ValueType::Any,
            return_julia_type: None,
            is_base_extension: false,
            type_params: vec![],
            vararg_param_index: None,
            vararg_fixed_count: None,
        });

        // Populate cache
        let _ = table.dispatch(&[JuliaType::Int64]);
        assert_eq!(table.dispatch_cache.borrow().len(), 1);

        // Add a more specific method — cache should be cleared
        table.add_method(MethodSig {
            _method_index: 1,
            global_index: 1,
            params: vec![("x".to_string(), JuliaType::Int64)],
            return_type: ValueType::I64,
            return_julia_type: None,
            is_base_extension: false,
            type_params: vec![],
            vararg_param_index: None,
            vararg_fixed_count: None,
        });
        assert!(
            table.dispatch_cache.borrow().is_empty(),
            "Cache should be cleared after add_method"
        );

        // Now dispatch should find the more specific method
        let r = table.dispatch(&[JuliaType::Int64]);
        assert!(r.is_ok());
        assert_eq!(r.unwrap().global_index, 1);
    }
}
