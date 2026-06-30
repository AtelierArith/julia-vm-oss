//! Higher-order function return type inference.
//!
//! Handles call-site specialization for HOFs like map, filter, and reduce
//! to infer their return types based on the function argument and collection type.

use crate::compile::lattice::types::{ConcreteType, LatticeType};
use crate::compile::tfuncs::{hof_ops, HofLambdaAnalyzer};
use crate::inference_core::CoreType;
use crate::ir::core::{Expr, Function, Stmt};
use crate::types::JuliaType;
use crate::vm::{ArrayElementType, ValueType};

use crate::compile::CoreCompiler;

/// Whether a `filter(pred, coll)` over a receiver of this `JuliaType` preserves a
/// struct-backed container type (`Dict{K,V}` / `Set{T}`). Used to propagate the
/// receiver's own type as the `filter` result so a `filter` binding routes
/// identically to the collection it came from (Issue #6672).
fn filter_preserves_struct_container(jt: &JuliaType) -> bool {
    matches!(jt, JuliaType::Struct(name) if {
        let short = name.rsplit('.').next().unwrap_or(name);
        let base = short.split('{').next().unwrap_or(short);
        matches!(base, "Dict" | "Set")
    })
}

/// `CoreCompiler` is the expression-inference side's
/// [`HofLambdaAnalyzer`] (Issue #6604): it re-enters the shared inference
/// engine to infer a HOF lambda's return type. This is the HOF counterpart of
/// the `StructInstantiation` `&mut` seam — it lets the registry-side `map`
/// rule (`tfuncs::hof_ops::map_call_result`) analyze the lambda *expression*
/// without the registry depending on `CoreCompiler`.
impl HofLambdaAnalyzer for CoreCompiler<'_> {
    fn map_mapped_element_type(
        &mut self,
        func_expr: &Expr,
        input_elements: &[LatticeType],
    ) -> Option<LatticeType> {
        // One element type per mapped collection: unary `map` passes one, binary
        // `broadcast`/`map` passes two, n-ary passes more (Issue #6604).
        let element_value_types: Vec<ValueType> =
            input_elements.iter().map(ValueType::from).collect();
        let mapped = self.map_callable_return_value_types(func_expr, &element_value_types)?;
        Some(LatticeType::from(&mapped))
    }

    fn reduce_result_type(&mut self, op_expr: &Expr, element: &LatticeType) -> Option<LatticeType> {
        let element_type = ValueType::from(element);
        let result = self.infer_reduce_operator_return_type(op_expr, element_type)?;
        Some(LatticeType::from(&result))
    }
}

impl CoreCompiler<'_> {
    /// Infer the return type of a `map(f, arr)` call at the call site.
    ///
    /// This performs call-site specialization for HOF type inference:
    /// - Extracts the element type from the array argument
    /// - Drives the registry's `map` rule
    ///   ([`hof_ops::map_call_result`]), which calls back into `self` (as a
    ///   [`HofLambdaAnalyzer`]) to infer the lambda's mapped element type from
    ///   the function-argument expression (Issue #6604)
    ///
    /// Returns `Some(ValueType)` if inference succeeds, `None` otherwise.
    pub(in crate::compile) fn infer_map_call_return_type(
        &mut self,
        func_arg: &Expr,
        arr_arg: &Expr,
    ) -> Option<ValueType> {
        let element_type = self.hof_iterable_element_type(arr_arg)?;

        // Build the lattice argument vector the registry rule expects:
        // `[f, collection]`. The collection's element type is recovered from
        // `hof_iterable_element_type` (which already understands range
        // literals, ArrayOf, and Array-with-JuliaType element types) so the
        // registry rule sees a concrete `Array{T}` even when the raw
        // expression inference would only yield a bare `Array`.
        let arg_types = [
            LatticeType::from(&self.infer_expr_type(func_arg)),
            array_collection(&element_type),
        ];

        let result = hof_ops::map_call_result(&arg_types, func_arg, self);
        array_result_to_value_type(&result)
    }

    /// Infer the mapped element type of a HOF callable applied to a single
    /// element of type `element_type` (Issue #6604).
    ///
    /// Shared by [`Self::infer_map_call_return_type`] (via the
    /// [`HofLambdaAnalyzer`] impl). Returns `None` when the callable or its
    /// return type cannot be resolved.
    fn map_callable_return_value_type(
        &mut self,
        func_arg: &Expr,
        element_type: &ValueType,
    ) -> Option<ValueType> {
        match self.resolve_hof_callable(func_arg) {
            Some(HofCallable::InlineFunction(func)) => {
                Some(self.infer_shared_function_return_type_with_arg_types(
                    &func,
                    std::slice::from_ref(element_type),
                ))
            }
            Some(HofCallable::Named(func_name)) => {
                self.infer_named_map_callable_return_type(&func_name, element_type)
            }
            None => None,
        }
    }

    /// Infer the mapped element type of a HOF callable applied to one element
    /// per mapped collection (Issue #6604), dispatching on arity so the unary,
    /// binary, and n-ary callable rules each keep their existing behaviour. This
    /// backs the [`HofLambdaAnalyzer`] impl for `map`/`broadcast` of any arity.
    fn map_callable_return_value_types(
        &mut self,
        func_arg: &Expr,
        element_types: &[ValueType],
    ) -> Option<ValueType> {
        match element_types {
            [] => None,
            [single] => self.map_callable_return_value_type(func_arg, single),
            [left, right] => match self.resolve_hof_callable(func_arg) {
                Some(HofCallable::InlineFunction(func)) => {
                    Some(self.infer_shared_function_return_type_with_arg_types(
                        &func,
                        &[left.clone(), right.clone()],
                    ))
                }
                Some(HofCallable::Named(func_name)) => {
                    self.infer_named_binary_map_callable_return_type(&func_name, left, right)
                }
                None => None,
            },
            many => self.infer_nary_map_callable_return_type(func_arg, many),
        }
    }

    /// Infer the return type of a binary element-wise HOF call such as
    /// `broadcast(f, left, right)` when both input element types are visible.
    pub(in crate::compile) fn infer_binary_map_call_return_type(
        &mut self,
        func_arg: &Expr,
        left_arg: &Expr,
        right_arg: &Expr,
    ) -> Option<ValueType> {
        let left_element_type = self.hof_iterable_element_type(left_arg)?;
        let right_element_type = self.hof_iterable_element_type(right_arg)?;
        let arg_types = [
            LatticeType::Top,
            array_collection(&left_element_type),
            array_collection(&right_element_type),
        ];
        let result = hof_ops::nary_map_call_result(&arg_types, func_arg, self)?;
        array_result_to_value_type(&result)
    }

    pub(in crate::compile) fn infer_binary_map_call_return_type_from_julia_types(
        &mut self,
        func_arg: &Expr,
        left_arg_type: &JuliaType,
        right_arg_type: &JuliaType,
    ) -> Option<ValueType> {
        let left_element_type = self.hof_iterable_element_type_from_julia_type(left_arg_type)?;
        let right_element_type = self.hof_iterable_element_type_from_julia_type(right_arg_type)?;
        let return_type = match self.resolve_hof_callable(func_arg) {
            Some(HofCallable::Named(func_name)) => self
                .infer_named_binary_map_callable_return_type(
                    &func_name,
                    &left_element_type,
                    &right_element_type,
                )?,
            _ => return None,
        };

        Some(ValueType::ArrayOf(
            map_return_element_type(&return_type),
            None,
        ))
    }

    pub(in crate::compile) fn infer_nary_map_call_return_type(
        &mut self,
        func_arg: &Expr,
        array_args: &[Expr],
    ) -> Option<ValueType> {
        let element_types = array_args
            .iter()
            .map(|arg| self.hof_iterable_element_type(arg))
            .collect::<Option<Vec<_>>>()?;
        let mut arg_types = Vec::with_capacity(element_types.len() + 1);
        arg_types.push(LatticeType::Top);
        arg_types.extend(element_types.iter().map(array_collection));
        let result = hof_ops::nary_map_call_result(&arg_types, func_arg, self)?;
        array_result_to_value_type(&result)
    }

    pub(in crate::compile) fn infer_nary_map_call_return_type_from_julia_types(
        &mut self,
        func_arg: &Expr,
        array_arg_types: &[JuliaType],
    ) -> Option<ValueType> {
        let element_types = array_arg_types
            .iter()
            .map(|ty| self.hof_iterable_element_type_from_julia_type(ty))
            .collect::<Option<Vec<_>>>()?;
        let return_type = self.infer_nary_map_callable_return_type(func_arg, &element_types)?;

        Some(ValueType::ArrayOf(
            map_return_element_type(&return_type),
            None,
        ))
    }

    /// Infer the return type of a `filter(pred, arr)` call at the call site.
    ///
    /// Filter returns an array with the same element type as the input.
    pub(in crate::compile) fn infer_filter_call_return_type(
        &mut self,
        arr_arg: &Expr,
    ) -> Option<ValueType> {
        // `filter` never changes the element type, so the registry rule
        // (`hof_ops::filter_call_result` → `tfunc_filter`) computes the result
        // for the element-bearing range case; bare `ArrayOf`/`Array` results
        // pass through unchanged to avoid a lossy element round-trip (Issue #6604).
        if let Some(element_type) = self.integer_range_literal_element_type_from_expr(arr_arg) {
            let arg_types = [LatticeType::Top, array_collection(&element_type)];
            return array_result_to_value_type(&hof_ops::filter_call_result(&arg_types));
        }

        match self.infer_expr_type(arr_arg) {
            ValueType::ArrayOf(elem, _) => Some(ValueType::ArrayOf(elem, None)),
            ValueType::Array => Some(ValueType::Array),
            // `filter(pred, coll)` only drops entries, so the container type is
            // preserved. Propagating the receiver's own type keeps a
            // `filtered = filter(p, d)` binding consistent with the dict/set it
            // came from. Without this the result widened to `Any`, and
            // collection-mutation routing (`empty!`/`merge!`/…) then treated the
            // result as a runtime-dispatch boundary, demoting `empty!(filtered)`
            // to a legacy `DictEmpty` instruction instead of native struct-backed
            // dispatch and failing the Issue #6621 guard (Issue #6672).
            native @ (ValueType::Dict | ValueType::Set) => Some(native),
            struct_ty @ ValueType::Struct(_)
                if filter_preserves_struct_container(&self.infer_julia_type(arr_arg)) =>
            {
                Some(struct_ty)
            }
            _ => None,
        }
    }

    /// Infer the return type of a `reduce(op, itr)` or `foldl/foldr` call at the call site.
    ///
    /// For reduce operations:
    /// - If the operator is a known function like `+`, `*`, etc., the return type
    ///   depends on the element type
    /// - For `+` and `*` on integers, the result is Int64
    /// - For `+` and `*` on floats, the result is Float64
    ///
    /// This enables proper type inference for `reduce(+, [1,2,3])` -> Int64
    pub(in crate::compile) fn infer_reduce_call_return_type(
        &mut self,
        op_arg: &Expr,
        itr_arg: &Expr,
    ) -> Option<ValueType> {
        let element_type = self.hof_iterable_element_type(itr_arg)?;
        let arg_types = [LatticeType::Top, array_collection(&element_type)];
        let result = hof_ops::reduce_call_result(&arg_types, op_arg, self)?;
        Some(ValueType::from(&result))
    }

    /// Infer the return type of a `mapreduce/mapfoldl/mapfoldr(f, op, itr)` call
    /// when the mapped element type and reducer are both visible at the call site.
    pub(in crate::compile) fn infer_mapreduce_call_return_type(
        &mut self,
        func_arg: &Expr,
        op_arg: &Expr,
        itr_arg: &Expr,
    ) -> Option<ValueType> {
        let element_type = self.hof_iterable_element_type(itr_arg)?;
        let arg_types = [
            LatticeType::Top,
            LatticeType::Top,
            array_collection(&element_type),
        ];
        let result = hof_ops::mapreduce_call_result(&arg_types, func_arg, op_arg, self)?;
        Some(ValueType::from(&result))
    }

    /// Infer the return type of a higher-order-function call (`map`,
    /// `broadcast`, `filter`, `reduce`, `foldl`, `foldr`, `mapreduce`,
    /// `mapfoldl`, `mapfoldr`) purely from the call-site argument expressions —
    /// inline-lambda or named callables and the visible collection element type.
    /// Returns `None` for non-HOF functions or when the call shape / element
    /// type makes call-site inference impossible.
    ///
    /// This consolidates the per-HOF dispatch so it can be reused on the
    /// runtime-dispatch fallback path. A hoisted inline-lambda argument now
    /// infers as `Any` because its bare nested name is no longer in the
    /// short-name method table (Issue #8105); that flips `has_any_arg`, so the
    /// HOF call dispatches through the `NoMethodFound`/runtime arm that would
    /// otherwise discard the statically inferable result type and store the
    /// binding as `Any`. Recovering the type here keeps `y = reduce(op, xs)`
    /// (and the rest of the HOF family) precisely typed regardless of whether
    /// the callable argument resolves statically.
    pub(in crate::compile) fn infer_hof_call_site_return_type(
        &mut self,
        function: &str,
        args: &[Expr],
    ) -> Option<ValueType> {
        let name = function.strip_prefix("Base.").unwrap_or(function);
        match name {
            "map" | "broadcast" => match args.len() {
                2 => self.infer_map_call_return_type(&args[0], &args[1]),
                3 => self.infer_binary_map_call_return_type(&args[0], &args[1], &args[2]),
                n if n >= 4 => self.infer_nary_map_call_return_type(&args[0], &args[1..]),
                _ => None,
            },
            "filter" if args.len() == 2 => self.infer_filter_call_return_type(&args[1]),
            "mapreduce" | "mapfoldl" | "mapfoldr" if args.len() >= 3 => {
                self.infer_mapreduce_call_return_type(&args[0], &args[1], &args[2])
            }
            "reduce" | "foldl" | "foldr" if args.len() >= 2 => {
                self.infer_reduce_call_return_type(&args[0], &args[1])
            }
            _ => None,
        }
    }

    fn infer_reduce_operator_return_type(
        &mut self,
        op_arg: &Expr,
        element_type: ValueType,
    ) -> Option<ValueType> {
        let op_name = match self.resolve_hof_callable(op_arg)? {
            HofCallable::InlineFunction(func) => {
                return Some(self.infer_shared_function_return_type_with_arg_types(
                    &func,
                    &[element_type.clone(), element_type.clone()],
                ));
            }
            HofCallable::Named(name) => name,
        };

        // For binary operators like +, *, -, etc., the result type is typically
        // the same as or promoted from the element type
        match op_name.as_str() {
            "min" | "max" => Some(element_type),
            "+" | "*" | "-" | "/" | "^" => {
                // Numeric operations preserve or promote the type
                match &element_type {
                    ValueType::I64 | ValueType::I32 => {
                        // Integer operations return integers (for +, *, -)
                        // Division / on integers may return Float64, but for inference
                        // we assume integer result for reduce context
                        if op_name == "/" {
                            Some(ValueType::F64)
                        } else {
                            Some(ValueType::I64)
                        }
                    }
                    // Julia widens Bool sums/products to Int64. The Pure Julia
                    // mapfoldl/mapreduce specializations preserve small integer
                    // widths, but Bool is the numeric exception (Issue #4619).
                    ValueType::Bool if op_name == "+" || op_name == "*" => Some(ValueType::I64),
                    ValueType::F64 | ValueType::F32 => Some(ValueType::F64),
                    _ => Some(element_type),
                }
            }
            "&" | "|" | "xor" => {
                // Bitwise operations on integers return integers
                Some(element_type)
            }
            _ => {
                // For user-defined operators, try to look up and infer
                if let Some(table) = self.method_tables.get(op_name.as_str()) {
                    // reduce(op, itr) calls op(acc, elem) where acc and elem are both element_type
                    let arg_julia_type = self.value_type_to_julia_type(&element_type);
                    if let Ok(method) = table.dispatch(&[arg_julia_type.clone(), arg_julia_type]) {
                        // If method return type is Any, try to re-infer
                        if matches!(&method.return_type, ValueType::Any) {
                            if let Some(func_ir) = self
                                .shared_ctx
                                .function_ir_by_global_index
                                .get(&method.global_index)
                            {
                                let inferred = self
                                    .infer_shared_function_return_type_with_arg_types(
                                        func_ir,
                                        &[element_type.clone(), element_type.clone()],
                                    );
                                return Some(inferred);
                            }
                        }
                        return Some(method.return_type.clone());
                    }
                }
                // Default to element type for unknown operators
                Some(element_type)
            }
        }
    }
}

enum HofCallable {
    Named(String),
    InlineFunction(Function),
}

impl CoreCompiler<'_> {
    fn hof_iterable_element_type(&mut self, iter_arg: &Expr) -> Option<ValueType> {
        if let Some(element_type) = self.integer_range_literal_element_type_from_expr(iter_arg) {
            return Some(element_type);
        }

        match self.infer_expr_type(iter_arg) {
            ValueType::ArrayOf(elem, _) => Some(map_input_element_value_type(&elem)),
            ValueType::Array => self
                .hof_iterable_element_type_from_julia_type(&self.infer_julia_type(iter_arg))
                .or(Some(ValueType::Any)),
            _ => None,
        }
    }

    fn hof_iterable_element_type_from_julia_type(
        &self,
        julia_type: &JuliaType,
    ) -> Option<ValueType> {
        match julia_type {
            JuliaType::VectorOf(element) | JuliaType::MatrixOf(element) => {
                let element = super::shared::array_element_type_for_julia_type(element, |name| {
                    self.shared_ctx.get_struct_type_id(name)
                })?;
                Some(map_input_element_value_type(&element))
            }
            _ => None,
        }
    }

    fn integer_range_literal_element_type_from_expr(&mut self, expr: &Expr) -> Option<ValueType> {
        let Expr::Range {
            start, step, stop, ..
        } = expr
        else {
            return None;
        };

        self.integer_range_literal_element_type(start, step.as_deref(), stop)
    }

    fn integer_range_literal_element_type(
        &mut self,
        start: &Expr,
        step: Option<&Expr>,
        stop: &Expr,
    ) -> Option<ValueType> {
        let start_type = self.infer_expr_type(start);
        let stop_type = self.infer_expr_type(stop);
        let step_type = step.map(|expr| self.infer_expr_type(expr));
        let step_is_i64 = step_type
            .as_ref()
            .is_none_or(|ty| matches!(ty, ValueType::I64));

        if matches!(start_type, ValueType::I64)
            && matches!(stop_type, ValueType::I64)
            && step_is_i64
        {
            Some(ValueType::I64)
        } else {
            None
        }
    }

    fn resolve_hof_callable(&self, func_arg: &Expr) -> Option<HofCallable> {
        match func_arg {
            Expr::FunctionRef { name, .. } | Expr::Var(name, _) => {
                Some(HofCallable::Named(name.clone()))
            }
            Expr::LetBlock { body, .. } => {
                let name = body.stmts.iter().rev().find_map(|stmt| match stmt {
                    Stmt::Expr {
                        expr: Expr::FunctionRef { name, .. },
                        ..
                    }
                    | Stmt::Expr {
                        expr: Expr::Var(name, _),
                        ..
                    } => Some(name.as_str()),
                    _ => None,
                })?;

                body.stmts
                    .iter()
                    .find_map(|stmt| match stmt {
                        Stmt::FunctionDef { func, .. } if func.name == name => {
                            Some(HofCallable::InlineFunction((**func).clone()))
                        }
                        _ => None,
                    })
                    .or_else(|| Some(HofCallable::Named(name.to_string())))
            }
            _ => None,
        }
    }

    fn infer_named_map_callable_return_type(
        &mut self,
        func_name: &str,
        element_type: &ValueType,
    ) -> Option<ValueType> {
        let normalized_func_name = func_name.strip_prefix("function ").unwrap_or(func_name);
        if matches!(
            normalized_func_name,
            "iszero" | "isone" | "signbit" | "iseven" | "isodd"
        ) {
            return Some(ValueType::Bool);
        }
        if matches!(normalized_func_name, "identity" | "abs" | "abs2" | "-") {
            return Some(element_type.clone());
        }

        let table = self.method_tables.get(func_name)?;
        let arg_julia_type = self.value_type_to_julia_type(element_type);
        let method = table.dispatch(&[arg_julia_type]).ok()?;

        if matches!(&method.return_type, ValueType::Any) {
            if let Some(func_ir) = self
                .shared_ctx
                .function_ir_by_global_index
                .get(&method.global_index)
            {
                return Some(self.infer_shared_function_return_type_with_arg_types(
                    func_ir,
                    std::slice::from_ref(element_type),
                ));
            }
        }

        Some(method.return_type.clone())
    }

    fn infer_named_binary_map_callable_return_type(
        &mut self,
        func_name: &str,
        left_element_type: &ValueType,
        right_element_type: &ValueType,
    ) -> Option<ValueType> {
        let normalized_func_name = func_name.strip_prefix("function ").unwrap_or(func_name);
        if let Some(return_type) = binary_numeric_map_return_type(
            normalized_func_name,
            left_element_type,
            right_element_type,
        ) {
            return Some(return_type);
        }

        let table = self.method_tables.get(func_name)?;
        let left_julia_type = self.value_type_to_julia_type(left_element_type);
        let right_julia_type = self.value_type_to_julia_type(right_element_type);
        let method = table.dispatch(&[left_julia_type, right_julia_type]).ok()?;

        if matches!(&method.return_type, ValueType::Any) {
            if let Some(func_ir) = self
                .shared_ctx
                .function_ir_by_global_index
                .get(&method.global_index)
            {
                return Some(self.infer_shared_function_return_type_with_arg_types(
                    func_ir,
                    &[left_element_type.clone(), right_element_type.clone()],
                ));
            }
        }

        Some(method.return_type.clone())
    }

    fn infer_nary_map_callable_return_type(
        &mut self,
        func_arg: &Expr,
        element_types: &[ValueType],
    ) -> Option<ValueType> {
        match self.resolve_hof_callable(func_arg) {
            Some(HofCallable::InlineFunction(func)) => {
                Some(self.infer_shared_function_return_type_with_arg_types(&func, element_types))
            }
            Some(HofCallable::Named(func_name)) => {
                infer_named_nary_map_callable_return_type(&func_name, element_types)
            }
            None => None,
        }
    }
}

/// Convert a `ValueType` element to a `ConcreteType` for an `Array{T}` lattice
/// element position (Issue #6604). Non-concrete value types (`Any`/`Top`)
/// degrade to `ConcreteType::Core(CoreType::Any)`, matching the registry rule's
/// element-preservation fallback.
fn value_type_to_array_concrete(
    element_type: &ValueType,
) -> crate::compile::lattice::types::ConcreteType {
    match LatticeType::from(element_type) {
        LatticeType::Concrete(concrete) => concrete,
        _ => crate::compile::lattice::types::ConcreteType::Core(CoreType::Any),
    }
}

/// Build the `Array{T}` lattice argument the registry HOF rules consume from a
/// value-inference element type (Issue #6604). The adapters do the richer
/// element extraction (ranges, `ArrayOf`, `Array`-with-`JuliaType`) first, then
/// hand the rule a concrete `Array{T}`.
fn array_collection(element_type: &ValueType) -> LatticeType {
    LatticeType::Concrete(ConcreteType::Array {
        element: Box::new(value_type_to_array_concrete(element_type)),
        ndims: None,
    })
}

/// Convert a registry HOF rule's `Array{U}` lattice result back into the
/// value-inference array type `ArrayOf(U)` (Issue #6604). A non-array result
/// (the rule declined, or widened to a scalar) yields `None`, matching the
/// adapters' previous fall-through behaviour.
fn array_result_to_value_type(result: &LatticeType) -> Option<ValueType> {
    match result {
        LatticeType::Concrete(ConcreteType::Array { element, .. }) => {
            let mapped = ValueType::from(&LatticeType::Concrete((**element).clone()));
            Some(ValueType::ArrayOf(map_return_element_type(&mapped), None))
        }
        _ => None,
    }
}

fn map_input_element_value_type(element_type: &ArrayElementType) -> ValueType {
    match element_type {
        ArrayElementType::I64 => ValueType::I64,
        ArrayElementType::F64 => ValueType::F64,
        ArrayElementType::I32 => ValueType::I32,
        ArrayElementType::F32 => ValueType::F32,
        ArrayElementType::Bool => ValueType::Bool,
        ArrayElementType::String => ValueType::Str,
        ArrayElementType::Char => ValueType::Char,
        _ => ValueType::Any,
    }
}

fn map_return_element_type(return_type: &ValueType) -> ArrayElementType {
    match return_type {
        ValueType::I64 => ArrayElementType::I64,
        ValueType::F64 => ArrayElementType::F64,
        ValueType::I32 => ArrayElementType::I32,
        ValueType::F32 => ArrayElementType::F32,
        ValueType::Bool => ArrayElementType::Bool,
        ValueType::Str => ArrayElementType::String,
        ValueType::Char => ArrayElementType::Char,
        _ => ArrayElementType::Any,
    }
}

fn binary_numeric_map_return_type(
    func_name: &str,
    left_element_type: &ValueType,
    right_element_type: &ValueType,
) -> Option<ValueType> {
    match (func_name, left_element_type, right_element_type) {
        ("+" | "-" | "*", ValueType::I64, ValueType::I64) => Some(ValueType::I64),
        ("+" | "-" | "*", ValueType::I32, ValueType::I32) => Some(ValueType::I32),
        ("+" | "-" | "*", ValueType::F64, ValueType::F64) => Some(ValueType::F64),
        ("+" | "-" | "*", ValueType::F32, ValueType::F32) => Some(ValueType::F32),
        ("+", ValueType::Bool, ValueType::Bool) => Some(ValueType::I64),
        ("*", ValueType::Bool, ValueType::Bool) => Some(ValueType::Bool),
        (
            "/",
            ValueType::I64 | ValueType::I32 | ValueType::Bool,
            ValueType::I64 | ValueType::I32 | ValueType::Bool,
        ) => Some(ValueType::F64),
        ("/", ValueType::F32, ValueType::F32) => Some(ValueType::F32),
        ("/", ValueType::F64, ValueType::F64) => Some(ValueType::F64),
        ("min" | "max", ValueType::I64, ValueType::I64) => Some(ValueType::I64),
        ("min" | "max", ValueType::I32, ValueType::I32) => Some(ValueType::I32),
        ("min" | "max", ValueType::F64, ValueType::F64) => Some(ValueType::F64),
        ("min" | "max", ValueType::F32, ValueType::F32) => Some(ValueType::F32),
        ("min" | "max", ValueType::Bool, ValueType::Bool) => Some(ValueType::Bool),
        _ => None,
    }
}

fn infer_named_nary_map_callable_return_type(
    func_name: &str,
    element_types: &[ValueType],
) -> Option<ValueType> {
    let normalized_func_name = func_name.strip_prefix("function ").unwrap_or(func_name);
    if !matches!(normalized_func_name, "+" | "*" | "min" | "max") || element_types.len() < 3 {
        return None;
    }
    let first = element_types.first()?;
    if !element_types.iter().all(|ty| ty == first) {
        return None;
    }

    match (normalized_func_name, first) {
        ("+" | "*", ValueType::I64) => Some(ValueType::I64),
        ("+" | "*", ValueType::I32) => Some(ValueType::I32),
        ("+" | "*", ValueType::F64) => Some(ValueType::F64),
        ("+" | "*", ValueType::F32) => Some(ValueType::F32),
        ("+", ValueType::Bool) => Some(ValueType::I64),
        ("*", ValueType::Bool) => Some(ValueType::Bool),
        ("min" | "max", ValueType::I64) => Some(ValueType::I64),
        ("min" | "max", ValueType::I32) => Some(ValueType::I32),
        ("min" | "max", ValueType::F64) => Some(ValueType::F64),
        ("min" | "max", ValueType::F32) => Some(ValueType::F32),
        ("min" | "max", ValueType::Bool) => Some(ValueType::Bool),
        _ => None,
    }
}
