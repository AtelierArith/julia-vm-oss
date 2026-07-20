//! Higher-order-function transfer rules that need the expression-reference
//! channel (Issue #6604).
//!
//! Plain [`TransferFn`](super::TransferFn)s such as
//! [`array_ops::tfunc_map`](super::array_ops::tfunc_map) only see argument
//! lattice types, so they can only sharpen `map(Float64, xs)` (a named
//! type-converter) and otherwise preserve the input element type. The precise
//! call-site rule — `map(x -> x * 2.0, Vector{Int}) :: Vector{Float64}` —
//! requires analyzing the lambda *expression*.
//!
//! This module hosts the registry-side rule for that case. It is driven by a
//! [`HofLambdaAnalyzer`] (implemented on the expression-inference side over
//! `CoreCompiler`) and consumed from the adapter
//! `compile::expr::infer::expr_tfuncs::infer_value_map_call`. Per the wave-2
//! caution recorded on the [`StructInstantiation`](super::registry) seam, the
//! rule is *not* wired into generic registry dispatch (which would over-match
//! engine call sites); it is a free function the adapter calls explicitly,
//! mirroring the `&mut` struct-instantiation seam.

use super::array_ops::{tfunc_filter, tfunc_map};
use super::registry::HofLambdaAnalyzer;
use crate::compile::lattice::types::{ConcreteType, LatticeType};
use crate::inference_core::CoreType;
use crate::ir::core::Expr;

/// Extract the concrete element type of an `Array{T}` lattice argument, or
/// `None` when the argument is not a concrete array (Issue #6604).
fn array_element(arg: Option<&LatticeType>) -> Option<LatticeType> {
    match arg {
        Some(LatticeType::Concrete(ConcreteType::Array { element, .. })) => {
            Some(LatticeType::Concrete((**element).clone()))
        }
        _ => None,
    }
}

/// Wrap a mapped element lattice type back into `Array{U}`, widening a
/// non-concrete mapped result (`Top`, i.e. `Any`) to `Array{Any}` — the
/// behaviour of the binary/n-ary map adapters, which always produce an array
/// when the callable resolves (Issue #6604).
fn array_of(element: LatticeType) -> LatticeType {
    let element = match element {
        LatticeType::Concrete(concrete) => concrete,
        _ => ConcreteType::Core(CoreType::Any),
    };
    LatticeType::Concrete(ConcreteType::Array {
        element: Box::new(element),
        ndims: None,
    })
}

/// Registry-level rule for `map(f, collection)` with the function-argument
/// expression available (Issue #6604).
///
/// `arg_types` are the lattice types of every argument (`[f, collection]`);
/// `func_expr` is the *syntax* of the first argument; `analyzer` re-enters the
/// shared inference engine to infer `f`'s return type on the collection's
/// element type.
///
/// When the analyzer can sharpen the mapped element type, the result is
/// `Array{U}`. Otherwise this falls back to the conservative
/// [`tfunc_map`] rule (named type-converter sharpening + element preservation),
/// so behavior is never *worse* than the plain transfer function.
pub fn map_call_result(
    arg_types: &[LatticeType],
    func_expr: &Expr,
    analyzer: &mut dyn HofLambdaAnalyzer,
) -> LatticeType {
    let fallback = tfunc_map(arg_types);

    // Only the unary `map(f, collection)` shape sharpens through the lambda
    // analyzer here; wider arities keep the conservative fallback.
    let Some(LatticeType::Concrete(ConcreteType::Array { element, .. })) = arg_types.get(1) else {
        return fallback;
    };

    let input_element = LatticeType::Concrete((**element).clone());
    match analyzer.map_mapped_element_type(func_expr, std::slice::from_ref(&input_element)) {
        Some(LatticeType::Concrete(mapped)) => LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(mapped),
            ndims: None,
        }),
        // The analyzer could not produce a concrete element type: keep the
        // conservative registry result rather than widening to `Array{Any}`.
        _ => fallback,
    }
}

/// Registry-level rule for binary/n-ary `map`/`broadcast` with the
/// function-argument expression available (Issue #6604).
///
/// `arg_types` are the lattice types of every argument (`[f, c1, c2, …]`); the
/// collections are `arg_types[1..]`. Each must be a concrete `Array{T}` (the
/// adapter is responsible for the richer element extraction — ranges,
/// `ArrayOf`, `Array`-with-`JuliaType` — before building these). The analyzer
/// infers the callable's return type on the per-collection element types; the
/// result is `Array{U}` (or `None` when the callable cannot be resolved or a
/// collection is not a concrete array).
pub fn nary_map_call_result(
    arg_types: &[LatticeType],
    func_expr: &Expr,
    analyzer: &mut dyn HofLambdaAnalyzer,
) -> Option<LatticeType> {
    let elements = arg_types
        .get(1..)?
        .iter()
        .map(|ty| array_element(Some(ty)))
        .collect::<Option<Vec<_>>>()?;
    if elements.is_empty() {
        return None;
    }
    let mapped = analyzer.map_mapped_element_type(func_expr, &elements)?;
    Some(array_of(mapped))
}

/// Registry-level rule for `filter(pred, collection)` (Issue #6604).
///
/// `filter` never changes the element type — the predicate's return type is
/// irrelevant to the result — so this needs no analyzer and simply defers to the
/// conservative [`tfunc_filter`] element-preserving rule.
pub fn filter_call_result(arg_types: &[LatticeType]) -> LatticeType {
    tfunc_filter(arg_types)
}

/// Registry-level rule for `reduce`/`foldl`/`foldr(op, collection)` with the
/// operator expression available (Issue #6604).
///
/// `arg_types` are `[op, collection]`. The analyzer's reduce-result rule infers
/// the scalar reduction type from the operator expression and the collection's
/// element type; returns `None` when the collection is not a concrete array or
/// the operator cannot be resolved.
pub fn reduce_call_result(
    arg_types: &[LatticeType],
    op_expr: &Expr,
    analyzer: &mut dyn HofLambdaAnalyzer,
) -> Option<LatticeType> {
    let element = array_element(arg_types.get(1))?;
    analyzer.reduce_result_type(op_expr, &element)
}

/// Registry-level rule for `mapreduce`/`mapfoldl`/`mapfoldr(f, op, collection)`
/// with both the mapper and operator expressions available (Issue #6604).
///
/// `arg_types` are `[f, op, collection]`. The mapper is applied to the
/// collection's element type (one analyzer call) and the operator is reduced
/// over the mapped element type (a second analyzer call); returns `None` when
/// the collection is not a concrete array or either callable cannot be resolved.
pub fn mapreduce_call_result(
    arg_types: &[LatticeType],
    func_expr: &Expr,
    op_expr: &Expr,
    analyzer: &mut dyn HofLambdaAnalyzer,
) -> Option<LatticeType> {
    let element = array_element(arg_types.get(2))?;
    let mapped = analyzer.map_mapped_element_type(func_expr, std::slice::from_ref(&element))?;
    analyzer.reduce_result_type(op_expr, &mapped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference_core::CorePrimitive;

    /// Stub analyzer that always maps to `Float64`, exercising the sharpening
    /// path without a full `CoreCompiler`.
    struct ConstFloatAnalyzer;

    impl HofLambdaAnalyzer for ConstFloatAnalyzer {
        fn map_mapped_element_type(
            &mut self,
            _func_expr: &Expr,
            _input_elements: &[LatticeType],
        ) -> Option<LatticeType> {
            Some(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Float64),
            )))
        }

        fn reduce_result_type(
            &mut self,
            _op_expr: &Expr,
            _element: &LatticeType,
        ) -> Option<LatticeType> {
            Some(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Float64),
            )))
        }
    }

    /// Stub analyzer that always declines (returns `None`).
    struct DeclineAnalyzer;

    impl HofLambdaAnalyzer for DeclineAnalyzer {
        fn map_mapped_element_type(
            &mut self,
            _func_expr: &Expr,
            _input_elements: &[LatticeType],
        ) -> Option<LatticeType> {
            None
        }

        fn reduce_result_type(
            &mut self,
            _op_expr: &Expr,
            _element: &LatticeType,
        ) -> Option<LatticeType> {
            None
        }
    }

    /// Stub analyzer with independently configurable map / reduce results, for
    /// exercising the `Any`-widening and two-step `mapreduce` paths.
    struct ConfigurableAnalyzer {
        map: Option<LatticeType>,
        reduce: Option<LatticeType>,
    }

    impl HofLambdaAnalyzer for ConfigurableAnalyzer {
        fn map_mapped_element_type(
            &mut self,
            _func_expr: &Expr,
            _input_elements: &[LatticeType],
        ) -> Option<LatticeType> {
            self.map.clone()
        }

        fn reduce_result_type(
            &mut self,
            _op_expr: &Expr,
            _element: &LatticeType,
        ) -> Option<LatticeType> {
            self.reduce.clone()
        }
    }

    fn int_array() -> LatticeType {
        LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ndims: None,
        })
    }

    fn func_arg() -> LatticeType {
        LatticeType::Concrete(ConcreteType::Function {
            name: "anonymous".to_string(),
        })
    }

    fn dummy_expr() -> Expr {
        Expr::Var(
            "f".to_string().into(),
            crate::span::Span::new(0, 0, 0, 0, 0, 0),
        )
    }

    #[test]
    fn analyzer_sharpens_mapped_element_type() {
        let args = vec![func_arg(), int_array()];
        let result = map_call_result(&args, &dummy_expr(), &mut ConstFloatAnalyzer);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Float64
                ))),
                ndims: None
            })
        );
    }

    #[test]
    fn declining_analyzer_falls_back_to_plain_tfunc_map() {
        // tfunc_map with an unknown callable preserves the input element type.
        let args = vec![func_arg(), int_array()];
        let result = map_call_result(&args, &dummy_expr(), &mut DeclineAnalyzer);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64
                ))),
                ndims: None
            })
        );
    }

    #[test]
    fn non_array_second_arg_falls_back() {
        // map over a non-array second arg: conservative `tfunc_map` → Top.
        let args = vec![func_arg(), LatticeType::Top];
        let result = map_call_result(&args, &dummy_expr(), &mut ConstFloatAnalyzer);
        assert_eq!(result, LatticeType::Top);
    }

    #[test]
    fn named_type_converter_still_sharpens_via_fallback() {
        // map(Float64, Vector{Int}): the conservative fallback already sharpens
        // a named type-converter, so even a declining analyzer yields Float64.
        let args = vec![
            LatticeType::Concrete(ConcreteType::DataType {
                name: "Float64".to_string(),
            }),
            int_array(),
        ];
        let result = map_call_result(&args, &dummy_expr(), &mut DeclineAnalyzer);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Float64
                ))),
                ndims: None
            })
        );
    }

    fn float64() -> LatticeType {
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )))
    }

    fn array_of_concrete(element: ConcreteType) -> LatticeType {
        LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(element),
            ndims: None,
        })
    }

    // --- nary_map_call_result (binary / n-ary map & broadcast) ---

    #[test]
    fn nary_map_sharpens_mapped_element_type() {
        let args = vec![func_arg(), int_array(), int_array()];
        let result = nary_map_call_result(&args, &dummy_expr(), &mut ConstFloatAnalyzer);
        assert_eq!(
            result,
            Some(array_of_concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            ))))
        );
    }

    #[test]
    fn nary_map_declines_when_analyzer_declines() {
        let args = vec![func_arg(), int_array(), int_array()];
        let result = nary_map_call_result(&args, &dummy_expr(), &mut DeclineAnalyzer);
        assert_eq!(result, None);
    }

    #[test]
    fn nary_map_widens_non_concrete_to_array_any() {
        // An inline lambda inferred as `Any` (Top) yields `Array{Any}`, matching
        // the binary/n-ary map adapters (unlike the unary `map` fallback, which
        // preserves the input element type).
        let mut analyzer = ConfigurableAnalyzer {
            map: Some(LatticeType::Top),
            reduce: None,
        };
        let args = vec![func_arg(), int_array(), int_array()];
        let result = nary_map_call_result(&args, &dummy_expr(), &mut analyzer);
        assert_eq!(
            result,
            Some(array_of_concrete(ConcreteType::Core(CoreType::Any)))
        );
    }

    #[test]
    fn nary_map_non_array_collection_declines() {
        let args = vec![func_arg(), int_array(), LatticeType::Top];
        let result = nary_map_call_result(&args, &dummy_expr(), &mut ConstFloatAnalyzer);
        assert_eq!(result, None);
    }

    // --- filter_call_result ---

    #[test]
    fn filter_preserves_element_type() {
        let args = vec![func_arg(), int_array()];
        let result = filter_call_result(&args);
        assert_eq!(
            result,
            array_of_concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    // --- reduce_call_result ---

    #[test]
    fn reduce_returns_scalar_result() {
        let args = vec![func_arg(), int_array()];
        let result = reduce_call_result(&args, &dummy_expr(), &mut ConstFloatAnalyzer);
        assert_eq!(result, Some(float64()));
    }

    #[test]
    fn reduce_declines_when_analyzer_declines() {
        let args = vec![func_arg(), int_array()];
        let result = reduce_call_result(&args, &dummy_expr(), &mut DeclineAnalyzer);
        assert_eq!(result, None);
    }

    #[test]
    fn reduce_non_array_collection_declines() {
        let args = vec![func_arg(), LatticeType::Top];
        let result = reduce_call_result(&args, &dummy_expr(), &mut ConstFloatAnalyzer);
        assert_eq!(result, None);
    }

    // --- mapreduce_call_result ---

    #[test]
    fn mapreduce_applies_map_then_reduce() {
        let mut analyzer = ConfigurableAnalyzer {
            map: Some(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Float64),
            ))),
            reduce: Some(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Float64),
            ))),
        };
        let args = vec![func_arg(), func_arg(), int_array()];
        let result = mapreduce_call_result(&args, &dummy_expr(), &dummy_expr(), &mut analyzer);
        assert_eq!(result, Some(float64()));
    }

    #[test]
    fn mapreduce_declines_when_map_declines() {
        let mut analyzer = ConfigurableAnalyzer {
            map: None,
            reduce: Some(float64()),
        };
        let args = vec![func_arg(), func_arg(), int_array()];
        let result = mapreduce_call_result(&args, &dummy_expr(), &dummy_expr(), &mut analyzer);
        assert_eq!(result, None);
    }

    #[test]
    fn mapreduce_declines_when_reduce_declines() {
        let mut analyzer = ConfigurableAnalyzer {
            map: Some(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Float64),
            ))),
            reduce: None,
        };
        let args = vec![func_arg(), func_arg(), int_array()];
        let result = mapreduce_call_result(&args, &dummy_expr(), &dummy_expr(), &mut analyzer);
        assert_eq!(result, None);
    }
}
