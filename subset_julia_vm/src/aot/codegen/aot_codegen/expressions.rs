use super::escape_rust_ident;
use super::global_static_ident;
use super::AotCodeGenerator;
use crate::aot::abi::AotAbiValue;
use crate::aot::ir::{AotBinOp, AotBuiltinOp, AotExpr, AotFunction, AotUnaryOp};
use crate::aot::types::StaticType;
use crate::aot::{AotError, AotResult, UnsupportedInstructionDiagnostic};

impl AotCodeGenerator {
    // ========== Expression Generation ==========

    fn emit_dict_constructor(
        &self,
        raw_args: &[AotExpr],
        return_ty: &StaticType,
    ) -> AotResult<String> {
        let StaticType::Dict { key, value } = return_ty else {
            return Err(AotError::CodegenError(format!(
                "Dict constructor requires Dict return type, got {} (Issue #7034)",
                return_ty
            )));
        };
        let key_ty = key.as_ref();
        let value_ty = value.as_ref();
        if raw_args.is_empty() {
            return Ok(format!(
                "std::collections::HashMap::<{}, {}>::new()",
                key_ty.to_rust_type(),
                value_ty.to_rust_type()
            ));
        }

        let mut code = format!(
            "{{ let mut _sjulia_dict = std::collections::HashMap::<{}, {}>::new();",
            key_ty.to_rust_type(),
            value_ty.to_rust_type()
        );
        for arg in raw_args {
            let AotExpr::TupleLit { elements } = arg else {
                return Err(AotError::CodegenError(
                    "AoT Dict construction expects Pair arguments lowered to two-element tuples (Issue #7034)"
                        .to_string(),
                ));
            };
            let [key_expr, value_expr] = elements.as_slice() else {
                return Err(AotError::CodegenError(
                    "AoT Dict construction expects each Pair to contain exactly key and value (Issue #7034)"
                        .to_string(),
                ));
            };
            let key_str = self.emit_value_for_binding_type(key_expr, key_ty, "Dict key")?;
            let value_str = self.emit_value_for_binding_type(value_expr, value_ty, "Dict value")?;
            code.push_str(&format!(
                " let _ = _sjulia_dict.insert({}, {});",
                key_str, value_str
            ));
        }
        code.push_str(" _sjulia_dict }");
        Ok(code)
    }

    fn emit_checked_1d_index(array: &str, index: &str) -> String {
        format!(
            "{{ let _sjulia_arr = &{}; let _sjulia_idx = {}; if _sjulia_idx < 1 || (_sjulia_idx as usize) > _sjulia_arr.len() {{ subset_julia_vm_runtime::error::aot_throw(format!(\"BoundsError({{:?}}, ({{}},))\", _sjulia_arr, _sjulia_idx)); }} _sjulia_arr[(_sjulia_idx - 1) as usize].clone() }}",
            array, index
        )
    }

    fn emit_checked_dict_index(dict: &str, key: &str) -> String {
        format!(
            "{{ let _sjulia_dict = &{}; let _sjulia_key = {}; _sjulia_dict.get(&_sjulia_key).cloned().unwrap_or_else(|| subset_julia_vm_runtime::error::aot_throw(format!(\"KeyError({{:?}})\", _sjulia_key))) }}",
            dict, key
        )
    }

    fn emit_checked_2d_index(array: &str, first: &str, second: &str) -> String {
        format!(
            "{{ let _sjulia_arr = &{}; let _sjulia_i = {}; let _sjulia_j = {}; if _sjulia_i < 1 || (_sjulia_i as usize) > _sjulia_arr.len() {{ subset_julia_vm_runtime::error::aot_throw(format!(\"BoundsError({{:?}}, ({{}}, {{}}))\", _sjulia_arr, _sjulia_i, _sjulia_j)); }} let _sjulia_row = &_sjulia_arr[(_sjulia_i - 1) as usize]; if _sjulia_j < 1 || (_sjulia_j as usize) > _sjulia_row.len() {{ subset_julia_vm_runtime::error::aot_throw(format!(\"BoundsError({{:?}}, ({{}}, {{}}))\", _sjulia_arr, _sjulia_i, _sjulia_j)); }} _sjulia_row[(_sjulia_j - 1) as usize].clone() }}",
            array, first, second
        )
    }

    fn emit_checked_nd_index(array: &str, indices: &[String]) -> String {
        let mut code = format!("{{ let _sjulia_arr_0 = &{};", array);
        let mut current = "_sjulia_arr_0".to_string();
        let mut emitted_indices = Vec::with_capacity(indices.len());
        for (dim, index) in indices.iter().enumerate() {
            let idx = format!("_sjulia_idx_{}", dim);
            emitted_indices.push("{}".to_string());
            code.push_str(&format!(
                " let {idx} = {index}; if {idx} < 1 || ({idx} as usize) > {current}.len() {{ subset_julia_vm_runtime::error::aot_throw(format!(\"BoundsError({{:?}}, ({}))\", _sjulia_arr_0, {})); }}",
                emitted_indices.join(", "),
                (0..=dim)
                    .map(|i| format!("_sjulia_idx_{}", i))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            if dim + 1 == indices.len() {
                code.push_str(&format!(" {current}[({idx} - 1) as usize].clone()"));
            } else {
                let next = format!("_sjulia_arr_{}", dim + 1);
                code.push_str(&format!(" let {next} = &{current}[({idx} - 1) as usize];"));
                current = next;
            }
        }
        code.push_str(" }");
        code
    }

    fn emit_checked_nd_linear_index(array: &str, index: &str, rank: usize) -> String {
        let mut code = format!("{{ let _sjulia_arr_0 = &{};", array);
        for dim in 0..rank {
            let parent = Self::nested_zero_index_expr("_sjulia_arr_0", dim);
            let empty_guard = (0..dim)
                .map(|prev| format!("_sjulia_dim_{prev} == 0usize"))
                .collect::<Vec<_>>()
                .join(" || ");
            if dim == 0 {
                code.push_str(" let _sjulia_dim_0 = _sjulia_arr_0.len();");
            } else if empty_guard.is_empty() {
                code.push_str(&format!(" let _sjulia_dim_{dim} = {parent}.len();"));
            } else {
                code.push_str(&format!(
                    " let _sjulia_dim_{dim} = if {empty_guard} {{ 0usize }} else {{ {parent}.len() }};"
                ));
            }
        }
        let len_expr = (0..rank)
            .map(|dim| format!("_sjulia_dim_{dim}"))
            .collect::<Vec<_>>()
            .join(" * ");
        code.push_str(&format!(
            " let _sjulia_linear_idx = {index}; let _sjulia_len = {len_expr}; if _sjulia_linear_idx < 1 || (_sjulia_linear_idx as usize) > _sjulia_len {{ subset_julia_vm_runtime::error::aot_throw(format!(\"BoundsError({{:?}}, ({{}},))\", _sjulia_arr_0, _sjulia_linear_idx)); }} let mut _sjulia_remaining = (_sjulia_linear_idx - 1) as usize;"
        ));
        for dim in 0..rank {
            code.push_str(&format!(
                " let _sjulia_idx_{dim} = _sjulia_remaining % _sjulia_dim_{dim}; _sjulia_remaining /= _sjulia_dim_{dim};"
            ));
        }
        let access = (0..rank).fold("_sjulia_arr_0".to_string(), |acc, dim| {
            format!("{acc}[_sjulia_idx_{dim}]")
        });
        code.push_str(&format!(" {access}.clone() }}"));
        code
    }

    fn emit_comprehension_iter_expr(&self, iter: &AotExpr) -> AotResult<String> {
        let iter_str = self.emit_expr_to_string(iter)?;
        Ok(match iter {
            AotExpr::Var { ty, .. } if ty.is_array() => format!("{}.iter().cloned()", iter_str),
            AotExpr::Var { ty, .. } if ty.is_set() => format!("{}.iter().cloned()", iter_str),
            AotExpr::Var { ty, .. } if ty.is_dict() => {
                format!(
                    "{}.iter().map(|(_sjulia_k, _sjulia_v)| (_sjulia_k.clone(), _sjulia_v.clone()))",
                    iter_str
                )
            }
            AotExpr::Var { ty, .. } if ty.is_range() => {
                format!("{}.clone().into_iter()", iter_str)
            }
            AotExpr::Range { .. } => format!("({}).into_iter()", iter_str),
            _ => iter_str,
        })
    }

    fn emit_owned_iter_expr(&self, iter: &AotExpr) -> AotResult<String> {
        let iter_str = self.emit_expr_to_string(iter)?;
        Ok(match iter {
            AotExpr::Var { ty, .. } if ty.is_array() => format!("{}.iter().cloned()", iter_str),
            AotExpr::Var { ty, .. } if ty.is_set() => format!("{}.iter().cloned()", iter_str),
            AotExpr::Var { ty, .. } if ty.is_dict() => {
                format!(
                    "{}.iter().map(|(_sjulia_k, _sjulia_v)| (_sjulia_k.clone(), _sjulia_v.clone()))",
                    iter_str
                )
            }
            AotExpr::Var { ty, .. } if ty.is_range() => {
                format!("{}.clone().into_iter()", iter_str)
            }
            AotExpr::Var { ty, .. } if ty.is_generator() => iter_str,
            AotExpr::ArrayLit { .. } | AotExpr::SetFromIter { .. } | AotExpr::Range { .. } => {
                format!("({}).into_iter()", iter_str)
            }
            _ => format!("({}).into_iter()", iter_str),
        })
    }

    fn emit_comprehension_loop_nest(
        &self,
        iterations: &[(String, AotExpr)],
        body: &AotExpr,
        filter: Option<&AotExpr>,
        depth: usize,
    ) -> AotResult<String> {
        let Some((var, iter)) = iterations.get(depth) else {
            if let Some(filter) = filter {
                let filter_str = self.emit_condition_expr(filter)?;
                let body_str = self.emit_expr_to_string(body)?;
                return Ok(format!(
                    "if {} {{ __sjulia_comp.push({}); }}",
                    filter_str, body_str
                ));
            }
            let body_str = self.emit_expr_to_string(body)?;
            return Ok(format!("__sjulia_comp.push({});", body_str));
        };

        let evar = escape_rust_ident(var);
        let iter_str = self.emit_comprehension_iter_expr(iter)?;
        let inner = self.emit_comprehension_loop_nest(iterations, body, filter, depth + 1)?;
        Ok(format!("for {} in {} {{ {} }}", evar, iter_str, inner))
    }

    fn emit_comprehension_expr(
        &self,
        body: &AotExpr,
        iterations: &[(String, AotExpr)],
        filter: Option<&AotExpr>,
        elem_ty: &StaticType,
    ) -> AotResult<String> {
        let elem_rust_ty = elem_ty.to_rust_type();
        let loops = self.emit_comprehension_loop_nest(iterations, body, filter, 0)?;
        Ok(format!(
            "{{ let mut __sjulia_comp: Vec<{}> = Vec::new(); {} __sjulia_comp }}",
            elem_rust_ty, loops
        ))
    }

    fn emit_generator_source_iter_expr(&self, iter: &AotExpr) -> AotResult<String> {
        let iter_str = self.emit_expr_to_string(iter)?;
        Ok(match iter {
            AotExpr::Var { ty, .. } if ty.is_array() => format!("{}.iter().cloned()", iter_str),
            AotExpr::Var { ty, .. } if ty.is_set() => format!("{}.iter().cloned()", iter_str),
            AotExpr::Var { ty, .. } if ty.is_range() => {
                format!("{}.clone().into_iter()", iter_str)
            }
            AotExpr::Var { ty, .. } if ty.is_generator() => iter_str,
            AotExpr::ArrayLit { .. } | AotExpr::SetFromIter { .. } | AotExpr::Range { .. } => {
                format!("({}).into_iter()", iter_str)
            }
            _ => format!("({}).into_iter()", iter_str),
        })
    }

    fn emit_generator_expr(
        &self,
        body: &AotExpr,
        var: &str,
        iter: &AotExpr,
        filter: Option<&AotExpr>,
        elem_ty: &StaticType,
    ) -> AotResult<String> {
        let evar = escape_rust_ident(var);
        let source = self.emit_generator_source_iter_expr(iter)?;
        let elem_rust_ty = elem_ty.to_rust_type();
        if let Some(filter) = filter {
            let filter_str = self.emit_condition_expr(filter)?;
            let body_str = self.emit_expr_to_string(body)?;
            Ok(format!(
                "Box::new({source}.filter_map(move |{evar}| {{ if {filter_str} {{ Some({body_str}) }} else {{ None }} }})) as Box<dyn Iterator<Item = {elem_rust_ty}>>"
            ))
        } else {
            let body_str = self.emit_expr_to_string(body)?;
            Ok(format!(
                "Box::new({source}.map(move |{evar}| {body_str})) as Box<dyn Iterator<Item = {elem_rust_ty}>>"
            ))
        }
    }

    fn emit_checked_2d_linear_index(array: &str, index: &str) -> String {
        format!(
            "{{ let _sjulia_arr = &{}; let _sjulia_linear_idx = {}; let _sjulia_rows = _sjulia_arr.len(); let _sjulia_cols = if _sjulia_arr.is_empty() {{ 0usize }} else {{ _sjulia_arr[0].len() }}; let _sjulia_len = _sjulia_rows * _sjulia_cols; if _sjulia_linear_idx < 1 || (_sjulia_linear_idx as usize) > _sjulia_len {{ subset_julia_vm_runtime::error::aot_throw(format!(\"BoundsError({{:?}}, ({{}},))\", _sjulia_arr, _sjulia_linear_idx)); }} let _sjulia_zero_idx = (_sjulia_linear_idx - 1) as usize; let _sjulia_row = _sjulia_zero_idx % _sjulia_rows; let _sjulia_col = _sjulia_zero_idx / _sjulia_rows; _sjulia_arr[_sjulia_row][_sjulia_col].clone() }}",
            array, index
        )
    }

    /// Emit expression and return as string
    pub(super) fn emit_expr_to_string(&self, expr: &AotExpr) -> AotResult<String> {
        match expr {
            // Literals
            AotExpr::LitI64(v) => Ok(format!("{}i64", v)),
            AotExpr::LitI32(v) => Ok(format!("{}i32", v)),
            AotExpr::LitF64(v) => {
                if v.is_nan() {
                    Ok("f64::NAN".to_string())
                } else if v.is_infinite() {
                    if *v > 0.0 {
                        Ok("f64::INFINITY".to_string())
                    } else {
                        Ok("f64::NEG_INFINITY".to_string())
                    }
                } else {
                    Ok(format!("{}_f64", v))
                }
            }
            AotExpr::LitF32(v) => {
                if v.is_nan() {
                    Ok("f32::NAN".to_string())
                } else if v.is_infinite() {
                    if *v > 0.0 {
                        Ok("f32::INFINITY".to_string())
                    } else {
                        Ok("f32::NEG_INFINITY".to_string())
                    }
                } else {
                    Ok(format!("{}_f32", v))
                }
            }
            AotExpr::LitBool(v) => Ok(format!("{}", v)),
            AotExpr::LitStr(s) => Ok(format!("\"{}\".to_string()", s.escape_default())),
            AotExpr::LitChar(c) => Ok(format!("'{}'", c.escape_default())),
            AotExpr::LitNothing => Ok("()".to_string()),
            AotExpr::LitMissing => Ok("Value::Missing".to_string()),

            // Variable
            AotExpr::Var { name, .. } => {
                // A reference to a top-level global emitted as a `static` is
                // rewritten to its collision-free `__sjulia_global_<name>` name,
                // unless a parameter of the enclosing function shadows it (in
                // which case the bare name is the parameter; Issue #7242).
                if self.global_names.contains(name)
                    && !self.current_function_param_names.contains(name)
                {
                    return Ok(global_static_ident(name));
                }
                // Julia's global `im` is emitted as a normal lowercase Rust const.
                // That lets local Julia bindings named `im` shadow it naturally
                // instead of forcing every `im` reference to an internal alias
                // (Issue #6966). `im` is not a `program.globals` entry, so it is
                // not rewritten above.
                Ok(escape_rust_ident(name))
            }

            // Binary operations
            AotExpr::BinOpStatic {
                op,
                left,
                right,
                result_ty,
            } => {
                let left_str = self.emit_expr_to_string(left)?;
                let right_str = self.emit_expr_to_string(right)?;
                let left_ty = left.get_type();
                let right_ty = right.get_type();

                self.emit_binop(*op, &left_str, &right_str, &left_ty, &right_ty, result_ty)
            }

            AotExpr::BinOpDynamic { op, left, right } => {
                if matches!(op, AotBinOp::Subtype) {
                    // Statically resolvable `<:` relations are const-folded in the
                    // IR converter (Issue #7037); reaching codegen means the
                    // operands are runtime type values, which are not supported.
                    return Err(AotError::UnsupportedInstruction(
                        UnsupportedInstructionDiagnostic::new(
                            "AoT codegen does not support subtype operator (<:) on runtime type values; only statically known type names are folded (Issue #7037)",
                        )
                        .with_workaround(
                            "use statically known type names so the relation can be const-folded, or run this check on the VM",
                        ),
                    ));
                }

                // `x == nothing` / `x != nothing` (incl. the `=== nothing` used by
                // the iteration protocol) is a `Value::Nothing` variant check in
                // the dynamic (`Value`) path — NOT a Rust unit comparison `x == ()`,
                // which is invalid Rust and flagged by the codegen-quality tests
                // (Issue #5658). Emit `(x).is_nothing()`.
                if matches!(
                    op,
                    AotBinOp::Eq | AotBinOp::Ne | AotBinOp::Egal | AotBinOp::NotEgal
                ) {
                    let other = if matches!(left.as_ref(), AotExpr::LitNothing) {
                        Some(self.emit_expr_to_string(right)?)
                    } else if matches!(right.as_ref(), AotExpr::LitNothing) {
                        Some(self.emit_expr_to_string(left)?)
                    } else {
                        None
                    };
                    if let Some(operand) = other {
                        let negate = matches!(op, AotBinOp::Ne | AotBinOp::NotEgal);
                        return Ok(format!(
                            "{}({}).is_nothing() /* dynamic */",
                            if negate { "!" } else { "" },
                            operand
                        ));
                    }
                }

                let left_str = self.emit_expr_as_value(left)?;
                let right_str = self.emit_expr_as_value(right)?;
                let runtime_op = Self::runtime_binop_variant(*op).ok_or_else(|| {
                    AotError::CodegenError(format!(
                        "AoT dynamic binary operation `{}` has no runtime dispatcher mapping",
                        op.to_rust_op()
                    ))
                })?;
                Ok(format!(
                    "subset_julia_vm_runtime::dynamic_binop(subset_julia_vm_runtime::BinOp::{}, &({}), &({})).unwrap()",
                    runtime_op, left_str, right_str
                ))
            }

            // Unary operations
            AotExpr::UnaryOp { op, operand, .. } => {
                let operand_str = self.emit_expr_to_string(operand)?;
                match op {
                    AotUnaryOp::Pos => Ok(operand_str), // +x is identity
                    _ => Ok(format!("{}{}", op.to_rust_op(), operand_str)),
                }
            }

            // Function calls
            AotExpr::CallStatic { function, args, .. } => {
                let args_str: Vec<_> = args
                    .iter()
                    .map(|a| self.emit_expr_to_string(a))
                    .collect::<AotResult<_>>()?;

                let resolved_name = if self.should_resolve_static_call(function) {
                    // Resolve multidispatch calls and true single-method user
                    // calls. The single-method case prevents no-method calls
                    // from leaking into Rust as invalid direct calls such as
                    // `only_string(1i64)` (Issue #7158), while preserving
                    // existing collapsed multi-method AoT E2E coverage.
                    let arg_types: Vec<_> = args.iter().map(|a| a.get_type()).collect();
                    self.resolve_dispatch(function, &arg_types)?
                } else {
                    AotFunction::sanitize_function_name(function)
                };

                Ok(format!("{}({})", resolved_name, args_str.join(", ")))
            }

            AotExpr::CallDynamic { function, args } => {
                let args_str: Vec<_> = args
                    .iter()
                    .map(|a| self.emit_expr_as_value(a))
                    .collect::<AotResult<_>>()?;
                if self.needs_dispatch(function) {
                    Ok(format!(
                        "{}({}).unwrap()",
                        AotFunction::sanitize_function_name(function),
                        args_str.join(", ")
                    ))
                } else {
                    Ok(format!(
                        "subset_julia_vm_runtime::dynamic_call(\"{}\", &[{}]).unwrap()",
                        function,
                        args_str.join(", ")
                    ))
                }
            }

            AotExpr::CallBuiltin {
                builtin,
                args,
                return_ty,
            } => {
                let args_str: Vec<_> = args
                    .iter()
                    .map(|a| self.emit_expr_to_string(a))
                    .collect::<AotResult<_>>()?;
                let arg_types: Vec<_> = args.iter().map(|a| a.get_type()).collect();
                self.emit_builtin_call(builtin, args, &args_str, &arg_types, return_ty)
            }

            // Array literal (1D or multidimensional)
            AotExpr::ArrayLit {
                elements, shape, ..
            } => {
                let elems_str: Vec<_> = elements
                    .iter()
                    .map(|e| self.emit_expr_to_string(e))
                    .collect::<AotResult<_>>()?;

                // Check dimensionality
                if shape.len() <= 1 {
                    // 1D array: simple vec![]
                    Ok(format!("vec![{}]", elems_str.join(", ")))
                } else if shape.contains(&0) {
                    // Any zero dimension: empty nested vec
                    let inner = "vec![]".to_string();
                    let result = (1..shape.len()).fold(inner, |acc, _| format!("vec![{}]", acc));
                    Ok(result)
                } else {
                    // N-dimensional array (2D, 3D, ...): nested Vec
                    // Julia stores column-major: element[i0,i1,...] =
                    //   elements[i0 + i1*shape[0] + i2*shape[0]*shape[1] + ...]
                    // Build nested vec so arr[i0][i1]...[i_{n-1}] indexes correctly
                    Ok(build_nested_vec_colmajor(&elems_str, shape, 0, 0))
                }
            }
            AotExpr::SetFromIter { iter, elem_ty } => Ok(format!(
                "{}.collect::<std::collections::HashSet<{}>>()",
                self.emit_owned_iter_expr(iter)?,
                elem_ty.to_rust_type()
            )),

            // Tuple literal
            AotExpr::TupleLit { elements } => {
                let elems_str: Vec<_> = elements
                    .iter()
                    .map(|e| self.emit_expr_to_string(e))
                    .collect::<AotResult<_>>()?;
                if elems_str.len() == 1 {
                    Ok(format!("({},)", elems_str[0]))
                } else {
                    Ok(format!("({})", elems_str.join(", ")))
                }
            }
            AotExpr::NamedTupleLit { fields } => {
                let elems_str: Vec<_> = fields
                    .iter()
                    .map(|(_, e)| self.emit_expr_to_string(e))
                    .collect::<AotResult<_>>()?;
                if elems_str.len() == 1 {
                    Ok(format!("({},)", elems_str[0]))
                } else {
                    Ok(format!("({})", elems_str.join(", ")))
                }
            }
            AotExpr::Comprehension {
                body,
                var,
                iter,
                filter,
                elem_ty,
            } => {
                let iterations = vec![(var.clone(), iter.as_ref().clone())];
                self.emit_comprehension_expr(body, &iterations, filter.as_deref(), elem_ty)
            }
            AotExpr::MultiComprehension {
                body,
                iterations,
                filter,
                elem_ty,
            } => self.emit_comprehension_expr(body, iterations, filter.as_deref(), elem_ty),
            AotExpr::Generator {
                body,
                var,
                iter,
                filter,
                elem_ty,
            } => self.emit_generator_expr(body, var, iter, filter.as_deref(), elem_ty),

            // Index (1D or multidimensional, or tuple)
            AotExpr::Index {
                array,
                indices,
                is_tuple,
                ..
            } => {
                let array_str = self.emit_expr_to_string(array)?;
                let array_ty = array.get_type();

                if indices.is_empty() {
                    // Empty indices - shouldn't happen, but handle gracefully
                    Ok(array_str)
                } else if *is_tuple && indices.len() == 1 {
                    // Tuple indexing: t[1] -> t.0 (Julia 1-indexed to Rust .0, .1, etc.)
                    // Rust tuple fields require a compile-time literal index. Dynamic tuple
                    // indexing needs a Julia-compatible Union/runtime representation first.
                    let tuple_len = match &array_ty {
                        StaticType::Tuple(elements) => elements.len(),
                        StaticType::NamedTuple(fields) => fields.len(),
                        other => {
                            return Err(AotError::CodegenError(format!(
                                "AoT tuple indexing requires a static tuple type, got {} \
                                 (Issue #6962)",
                                other
                            )))
                        }
                    };
                    let AotExpr::LitI64(idx) = &indices[0] else {
                        return Err(AotError::CodegenError(
                            "AoT tuple indexing requires a constant integer index; dynamic \
                             t[i] needs Union/runtime tuple indexing support (Issue #6962)"
                                .to_string(),
                        ));
                    };
                    if *idx < 1 || (*idx as usize) > tuple_len {
                        return Err(AotError::CodegenError(format!(
                            "AoT tuple index {} is out of bounds for tuple length {} \
                             (Issue #6962)",
                            idx, tuple_len
                        )));
                    }
                    let rust_idx = *idx - 1;
                    Ok(format!("{}.{}", array_str, rust_idx))
                } else if indices.len() == 1 {
                    // 1D array indexing or N-D linear indexing: arr[i]
                    let index_str = self.emit_expr_to_string(&indices[0])?;
                    match array_ty {
                        StaticType::Dict { .. } => {
                            Ok(Self::emit_checked_dict_index(&array_str, &index_str))
                        }
                        StaticType::Array { ndims: Some(2), .. } => {
                            Ok(Self::emit_checked_2d_linear_index(&array_str, &index_str))
                        }
                        StaticType::Array {
                            ndims: Some(rank), ..
                        } if rank > 2 => Ok(Self::emit_checked_nd_linear_index(
                            &array_str, &index_str, rank,
                        )),
                        _ => Ok(Self::emit_checked_1d_index(&array_str, &index_str)),
                    }
                } else {
                    if let StaticType::Array {
                        ndims: Some(rank), ..
                    } = array_ty
                    {
                        if indices.len() != rank {
                            return Err(AotError::CodegenError(format!(
                                "AoT indexing for {}D arrays requires {} indices, got {} \
                                 (Issue #7033)",
                                rank,
                                rank,
                                indices.len()
                            )));
                        }
                    }
                    let index_strs: Vec<_> = indices
                        .iter()
                        .map(|index| self.emit_expr_to_string(index))
                        .collect::<AotResult<_>>()?;
                    if index_strs.len() == 2 {
                        Ok(Self::emit_checked_2d_index(
                            &array_str,
                            &index_strs[0],
                            &index_strs[1],
                        ))
                    } else {
                        Ok(Self::emit_checked_nd_index(&array_str, &index_strs))
                    }
                }
            }

            // Range
            AotExpr::Range {
                start,
                stop,
                step,
                elem_ty,
            } => self.emit_range_expr_to_string(start, stop, step.as_deref(), elem_ty),

            // Struct construction
            AotExpr::StructNew { name, fields } => {
                let fields_str: Vec<_> = fields
                    .iter()
                    .map(|f| self.emit_expr_to_string(f))
                    .collect::<AotResult<_>>()?;
                let constructor = Self::struct_constructor_rust_path(name);
                Ok(format!("{}::new({})", constructor, fields_str.join(", ")))
            }

            // Field access
            AotExpr::FieldAccess { object, field, .. } => {
                if matches!(object.get_type(), StaticType::DataType) {
                    return Err(AotError::UnsupportedInstruction(
                        UnsupportedInstructionDiagnostic::new(format!(
                            "AoT codegen does not support DataType field access `.{}` yet",
                            field
                        ))
                        .with_workaround(
                            "avoid reflecting on typeof(...) in AoT code; run through the VM until AoT has a full DataType object model (Issue #7068)",
                        ),
                    ));
                }
                let obj_str = self.emit_expr_to_string(object)?;
                Ok(format!("{}.{}", obj_str, field))
            }

            // Ternary
            AotExpr::Ternary {
                condition,
                then_expr,
                else_expr,
                result_ty,
            } => {
                let cond_str = self.emit_condition_expr(condition)?;
                let (then_str, else_str) =
                    if AotAbiValue::from_static_type(result_ty).needs_runtime_value() {
                        (
                            self.emit_expr_as_value(then_expr)?,
                            self.emit_expr_as_value(else_expr)?,
                        )
                    } else {
                        (
                            self.emit_expr_to_string(then_expr)?,
                            self.emit_expr_to_string(else_expr)?,
                        )
                    };
                Ok(format!(
                    "if {} {{ {} }} else {{ {} }}",
                    cond_str, then_str, else_str
                ))
            }

            // Boxing
            AotExpr::Box(inner) => {
                let inner_str = self.emit_expr_to_string(inner)?;
                Ok(format!("Box::new({})", inner_str))
            }

            // Unboxing
            AotExpr::Unbox { value, target_ty } => {
                let value_str = self.emit_expr_to_string(value)?;
                let ty_str = self.type_to_rust(target_ty);
                Ok(format!("*{} as {}", value_str, ty_str))
            }

            // Type conversion/coercion
            AotExpr::Convert { value, target_ty } => {
                let value_str = self.emit_expr_to_string(value)?;
                let value_ty = value.get_type();

                // Handle type conversions appropriately.
                match (&value_ty, target_ty) {
                    // Same type - no conversion needed
                    (a, b) if a == b => Ok(value_str),

                    (_, StaticType::Char) => Err(AotError::CodegenError(
                        "AoT codegen cannot lower conversion to Julia Char through Rust `char`; \
                         Julia Char can represent invalid Unicode code points that Rust rejects \
                         (Issue #6967)"
                            .to_string(),
                    )),

                    (_, target) if AotAbiValue::from_static_type(target).needs_runtime_value() => {
                        self.emit_expr_as_value(value)
                    }

                    // Bool to numeric
                    (StaticType::Bool, StaticType::I64)
                    | (StaticType::Bool, StaticType::I128)
                    | (StaticType::Bool, StaticType::I32)
                    | (StaticType::Bool, StaticType::I16)
                    | (StaticType::Bool, StaticType::I8)
                    | (StaticType::Bool, StaticType::U64)
                    | (StaticType::Bool, StaticType::U128)
                    | (StaticType::Bool, StaticType::U32)
                    | (StaticType::Bool, StaticType::U16)
                    | (StaticType::Bool, StaticType::U8)
                    | (StaticType::Bool, StaticType::F64)
                    | (StaticType::Bool, StaticType::F32) => match target_ty {
                        StaticType::F64 | StaticType::F32 => Ok(format!(
                            "({} as u8 as {})",
                            value_str,
                            self.type_to_rust(target_ty)
                        )),
                        _ => Ok(format!(
                            "({} as {})",
                            value_str,
                            self.type_to_rust(target_ty)
                        )),
                    },

                    (source, target) if Self::can_emit_unchecked_rust_cast(source, target) => {
                        Ok(format!("({} as {})", value_str, self.type_to_rust(target)))
                    }

                    (StaticType::Char, target) if Self::char_integer_target_fits(target) => Ok(
                        format!("({} as u32 as {})", value_str, self.type_to_rust(target)),
                    ),

                    // numeric → Bool: only 0 / 1 are exact (Issue #7038).
                    (source, StaticType::Bool)
                        if Self::is_float32_or_float64(source)
                            || Self::integer_layout(source).is_some() =>
                    {
                        Ok(self.emit_checked_to_bool(&value_str, source))
                    }

                    // float → integer: round-trip check throws InexactError for
                    // non-integer or out-of-range values (Issue #7038).
                    (source, target)
                        if Self::is_float32_or_float64(source)
                            && Self::integer_layout(target).is_some() =>
                    {
                        Ok(self.emit_checked_float_to_int(&value_str, source, target))
                    }

                    // integer → integer narrowing / sign boundary: `try_from`
                    // throws InexactError when the value does not fit (Issue #7038).
                    (source, target)
                        if Self::integer_layout(source).is_some()
                            && Self::integer_layout(target).is_some() =>
                    {
                        Ok(self.emit_checked_int_narrowing(&value_str, target))
                    }

                    _ => Err(Self::unsupported_checked_conversion(&value_ty, target_ty)),
                }
            }

            // Lambda/closure expression
            AotExpr::Lambda {
                params,
                body,
                captures,
                return_ty,
            } => self.emit_lambda(params, body, captures, return_ty),
        }
    }

    pub(super) fn emit_expr_as_value(&self, expr: &AotExpr) -> AotResult<String> {
        if let AotExpr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } = expr
        {
            let cond_str = self.emit_condition_expr(condition)?;
            let then_str = self.emit_expr_as_value(then_expr)?;
            let else_str = self.emit_expr_as_value(else_expr)?;
            return Ok(format!(
                "if {} {{ {} }} else {{ {} }}",
                cond_str, then_str, else_str
            ));
        }

        let expr_str = self.emit_expr_to_string(expr)?;
        let expr_ty = expr.get_type();
        if AotAbiValue::from_static_type(&expr_ty).needs_runtime_value() {
            Ok(expr_str)
        } else {
            Ok(format!("Value::from({})", expr_str))
        }
    }

    fn hof_array_element_type(expr: &AotExpr) -> StaticType {
        match expr.get_type() {
            StaticType::Array { element, .. }
            | StaticType::Range { element }
            | StaticType::Generator { element } => element.as_ref().clone(),
            _ => StaticType::Any,
        }
    }

    fn hof_operator_token(name: &str) -> Option<&'static str> {
        match name {
            "+" | "op_add" => Some("+"),
            "*" | "op_mul" => Some("*"),
            "-" | "op_sub" => Some("-"),
            "/" | "op_div" => Some("/"),
            _ => None,
        }
    }

    fn hof_function_expr(
        &self,
        function: &AotExpr,
        param_types: &[StaticType],
    ) -> AotResult<String> {
        let emitted = self.emit_expr_to_string(function)?;
        if Self::is_closure_literal(&emitted) {
            return Ok(emitted);
        }

        let name = match function {
            AotExpr::Var { name, .. } => name.as_str(),
            _ => return Ok(emitted),
        };
        if Self::hof_operator_token(name).is_some() {
            return Ok(name.to_string());
        }
        if self.needs_dispatch(name) {
            self.resolve_dispatch(name, param_types)
        } else {
            Ok(AotFunction::sanitize_function_name(name))
        }
    }

    fn emit_hof_unary_call(function: &str, arg: &str) -> String {
        if Self::is_closure_literal(function) {
            format!("({function})({arg})")
        } else {
            format!("{function}({arg})")
        }
    }

    fn emit_hof_binary_call(function: &str, lhs: &str, rhs: &str) -> String {
        if let Some(op) = Self::hof_operator_token(function) {
            format!("{lhs} {op} {rhs}")
        } else if Self::is_closure_literal(function) {
            format!("({function})({lhs}, {rhs})")
        } else {
            format!("{function}({lhs}, {rhs})")
        }
    }

    fn emit_range_expr_to_string(
        &self,
        start: &AotExpr,
        stop: &AotExpr,
        step: Option<&AotExpr>,
        elem_ty: &StaticType,
    ) -> AotResult<String> {
        if matches!(elem_ty, StaticType::Char) {
            if step.is_some() {
                return Err(AotError::CodegenError(
                    "AoT lazy Char range codegen supports unit-step ranges only (Issue #7039)"
                        .to_string(),
                ));
            }
            let start_str = self.emit_expr_as_static_type(start, elem_ty)?;
            let stop_str = self.emit_expr_as_static_type(stop, elem_ty)?;
            return Ok(format!("SjuliaCharRange::new({start_str}, {stop_str})"));
        }

        let start_str = self.emit_expr_as_static_type(start, elem_ty)?;
        let stop_str = self.emit_expr_as_static_type(stop, elem_ty)?;
        let step_str = match step {
            Some(step_expr) => self.emit_expr_as_static_type(step_expr, elem_ty)?,
            None => Self::range_unit_step_literal(elem_ty)?.to_string(),
        };
        let zero = Self::range_zero_literal(elem_ty)?;

        if elem_ty.is_signed() || elem_ty.is_unsigned() || Self::is_float32_or_float64(elem_ty) {
            Ok(format!(
                "{{ let _sjulia_range_step = {step_str}; if _sjulia_range_step == {zero} {{ subset_julia_vm_runtime::error::aot_throw(\"ArgumentError: step cannot be zero\"); }} SjuliaRange::new({start_str}, {stop_str}, _sjulia_range_step) }}"
            ))
        } else {
            Err(AotError::CodegenError(format!(
                "AoT lazy range expression codegen supports integer, Float32/Float64, and Char ranges, got {} (Issue #7039)",
                elem_ty
            )))
        }
    }

    pub(super) fn emit_expr_as_static_type(
        &self,
        expr: &AotExpr,
        target_ty: &StaticType,
    ) -> AotResult<String> {
        if &expr.get_type() == target_ty {
            self.emit_expr_to_string(expr)
        } else {
            self.emit_expr_to_string(&AotExpr::Convert {
                value: Box::new(expr.clone()),
                target_ty: target_ty.clone(),
            })
        }
    }

    fn runtime_binop_variant(op: AotBinOp) -> Option<&'static str> {
        Some(match op {
            AotBinOp::Add => "Add",
            AotBinOp::Sub => "Sub",
            AotBinOp::Mul => "Mul",
            AotBinOp::Div => "Div",
            AotBinOp::IntDiv => "IntDiv",
            AotBinOp::Mod => "Mod",
            AotBinOp::Pow => "Pow",
            AotBinOp::Lt => "Lt",
            AotBinOp::Gt => "Gt",
            AotBinOp::Le => "Le",
            AotBinOp::Ge => "Ge",
            AotBinOp::Eq | AotBinOp::Egal => "Eq",
            AotBinOp::Ne | AotBinOp::NotEgal => "Ne",
            AotBinOp::And => "And",
            AotBinOp::Or => "Or",
            AotBinOp::BitAnd => "BitAnd",
            AotBinOp::BitOr => "BitOr",
            AotBinOp::BitXor => "BitXor",
            AotBinOp::Shl => "Shl",
            AotBinOp::Shr => "Shr",
            AotBinOp::Subtype => return None,
        })
    }

    fn range_unit_step_literal(elem_ty: &StaticType) -> AotResult<&'static str> {
        match elem_ty {
            StaticType::I8 => Ok("1i8"),
            StaticType::I16 => Ok("1i16"),
            StaticType::I32 => Ok("1i32"),
            StaticType::I64 => Ok("1i64"),
            StaticType::I128 => Ok("1i128"),
            StaticType::U8 => Ok("1u8"),
            StaticType::U16 => Ok("1u16"),
            StaticType::U32 => Ok("1u32"),
            StaticType::U64 => Ok("1u64"),
            StaticType::U128 => Ok("1u128"),
            StaticType::F32 => Ok("1.0_f32"),
            StaticType::F64 => Ok("1.0_f64"),
            _ => Err(AotError::CodegenError(format!(
                "AoT range expression codegen does not support element type {} (Issue #6969)",
                elem_ty
            ))),
        }
    }

    fn range_zero_literal(elem_ty: &StaticType) -> AotResult<&'static str> {
        match elem_ty {
            StaticType::I8 => Ok("0i8"),
            StaticType::I16 => Ok("0i16"),
            StaticType::I32 => Ok("0i32"),
            StaticType::I64 => Ok("0i64"),
            StaticType::I128 => Ok("0i128"),
            StaticType::U8 => Ok("0u8"),
            StaticType::U16 => Ok("0u16"),
            StaticType::U32 => Ok("0u32"),
            StaticType::U64 => Ok("0u64"),
            StaticType::U128 => Ok("0u128"),
            StaticType::F32 => Ok("0.0_f32"),
            StaticType::F64 => Ok("0.0_f64"),
            _ => Err(AotError::CodegenError(format!(
                "AoT range expression codegen does not support element type {} (Issue #6969)",
                elem_ty
            ))),
        }
    }

    fn can_emit_unchecked_rust_cast(source: &StaticType, target: &StaticType) -> bool {
        (Self::is_float32_or_float64(source) && Self::is_float32_or_float64(target))
            || (Self::is_integer_without_bool(source) && Self::is_float32_or_float64(target))
            || Self::is_integer_cast_that_cannot_throw(source, target)
    }

    fn is_float32_or_float64(ty: &StaticType) -> bool {
        matches!(ty, StaticType::F32 | StaticType::F64)
    }

    fn is_integer_without_bool(ty: &StaticType) -> bool {
        Self::integer_layout(ty).is_some()
    }

    fn is_integer_cast_that_cannot_throw(source: &StaticType, target: &StaticType) -> bool {
        let Some((source_signed, source_bits)) = Self::integer_layout(source) else {
            return false;
        };
        let Some((target_signed, target_bits)) = Self::integer_layout(target) else {
            return false;
        };

        if source_signed == target_signed {
            return target_bits >= source_bits;
        }

        !source_signed && target_signed && target_bits > source_bits
    }

    fn integer_layout(ty: &StaticType) -> Option<(bool, u16)> {
        match ty {
            StaticType::I8 => Some((true, 8)),
            StaticType::I16 => Some((true, 16)),
            StaticType::I32 => Some((true, 32)),
            StaticType::I64 => Some((true, 64)),
            StaticType::I128 => Some((true, 128)),
            StaticType::U8 => Some((false, 8)),
            StaticType::U16 => Some((false, 16)),
            StaticType::U32 => Some((false, 32)),
            StaticType::U64 => Some((false, 64)),
            StaticType::U128 => Some((false, 128)),
            _ => None,
        }
    }

    fn char_integer_target_fits(target: &StaticType) -> bool {
        matches!(
            target,
            StaticType::I32
                | StaticType::I64
                | StaticType::I128
                | StaticType::U32
                | StaticType::U64
                | StaticType::U128
        )
    }

    /// Render a `contains` / `starts_with` / `ends_with` pattern argument: a
    /// `String` needs `.as_str()`, a `Char` is already a Rust `Pattern`
    /// (Issue #7058).
    fn string_pattern_arg(arg: &str, ty: Option<&StaticType>) -> String {
        if matches!(ty, Some(StaticType::Char)) {
            arg.to_string()
        } else {
            format!("({}).as_str()", arg)
        }
    }

    fn struct_constructor_rust_path(name: &str) -> String {
        if let Some(param) = StaticType::complex_param_rust_type_name(name) {
            format!("Complex::<{}>", param)
        } else if let Some(path) = StaticType::parametric_rust_constructor_path(name) {
            path
        } else {
            name.to_string()
        }
    }

    fn unsupported_checked_conversion(source: &StaticType, target: &StaticType) -> AotError {
        AotError::CodegenError(format!(
            "AoT codegen cannot lower checked Julia conversion from {} to {} with Rust `as`; \
             this conversion may require InexactError-compatible runtime checks (Issue #6968)",
            source, target
        ))
    }

    /// How a numeric value is rendered inside an `InexactError` message: floats
    /// use Julia float formatting (`1.0e30`), integers print directly.
    fn inexact_value_render(value_var: &str, source: &StaticType) -> String {
        match source {
            StaticType::F64 => format!("__sjulia_format_float64({})", value_var),
            StaticType::F32 => format!("__sjulia_format_float32({})", value_var),
            _ => value_var.to_string(),
        }
    }

    /// Emit a checked `float → integer` conversion (Issue #7038). A single
    /// round-trip test `(v as T) as F == v` rejects non-integer, out-of-range,
    /// NaN and Inf inputs in one shot, matching Julia's `InexactError: T(v)`.
    fn emit_checked_float_to_int(
        &self,
        value_str: &str,
        source: &StaticType,
        target: &StaticType,
    ) -> String {
        let rust_target = self.type_to_rust(target);
        let rust_source = self.type_to_rust(source);
        let julia_target = target.julia_type_name();
        let rendered = Self::inexact_value_render("_sjulia_v", source);
        format!(
            "{{ let _sjulia_v = {value}; let _sjulia_c = _sjulia_v as {t}; \
             if (_sjulia_c as {s}) == _sjulia_v {{ _sjulia_c }} else {{ \
             subset_julia_vm_runtime::error::aot_throw(format!(\"InexactError: {jt}({{}})\", {rendered})) }} }}",
            value = value_str,
            t = rust_target,
            s = rust_source,
            jt = julia_target,
            rendered = rendered,
        )
    }

    /// Emit a checked `integer → integer` narrowing / sign conversion via
    /// `try_from`, throwing `InexactError: trunc(T, v)` on failure (Issue #7038).
    fn emit_checked_int_narrowing(&self, value_str: &str, target: &StaticType) -> String {
        let rust_target = self.type_to_rust(target);
        let julia_target = target.julia_type_name();
        format!(
            "{{ let _sjulia_v = {value}; match {t}::try_from(_sjulia_v) {{ \
             Ok(_sjulia_x) => _sjulia_x, \
             Err(_) => subset_julia_vm_runtime::error::aot_throw(format!(\"InexactError: trunc({jt}, {{}})\", _sjulia_v)) }} }}",
            value = value_str,
            t = rust_target,
            jt = julia_target,
        )
    }

    /// Emit a checked `numeric → Bool` conversion: only `0`/`1` are exact,
    /// anything else throws `InexactError: Bool(v)` (Issue #7038).
    fn emit_checked_to_bool(&self, value_str: &str, source: &StaticType) -> String {
        let (zero, one) = if Self::is_float32_or_float64(source) {
            ("0.0", "1.0")
        } else {
            ("0", "1")
        };
        let rendered = Self::inexact_value_render("_sjulia_v", source);
        format!(
            "{{ let _sjulia_v = {value}; if _sjulia_v == {zero} {{ false }} else if _sjulia_v == {one} {{ true }} else {{ \
             subset_julia_vm_runtime::error::aot_throw(format!(\"InexactError: Bool({{}})\", {rendered})) }} }}",
            value = value_str,
            zero = zero,
            one = one,
            rendered = rendered,
        )
    }

    /// Emit lambda/closure expression
    ///
    /// Generates Rust closure syntax from Julia lambda expressions.
    ///
    /// # Examples
    /// ```ignore
    /// // Julia: x -> x + 1
    /// // Rust: |x: i64| -> i64 { x + 1i64 }
    ///
    /// // Julia: (x, y) -> x + y
    /// // Rust: |x: i64, y: i64| -> i64 { (x + y) }
    ///
    /// // Julia closure with capture:
    /// // let a = 10; f = x -> x + a
    /// // Rust: move |x: i64| -> i64 { (x + a) }
    /// ```
    fn emit_lambda(
        &self,
        params: &[(String, StaticType)],
        body: &AotExpr,
        captures: &[(String, StaticType)],
        return_ty: &StaticType,
    ) -> AotResult<String> {
        // Build parameter list with types
        let params_str: Vec<String> = params
            .iter()
            .map(|(name, ty)| format!("{}: {}", name, self.type_to_rust(ty)))
            .collect();

        // Generate return type
        let ret_ty_str = self.type_to_rust(return_ty);

        // Generate body expression
        let body_str = self.emit_expr_to_string(body)?;

        // Use 'move' if there are captured variables
        let move_keyword = if !captures.is_empty() { "move " } else { "" };

        // Generate closure syntax
        Ok(format!(
            "{}|{}| -> {} {{ {} }}",
            move_keyword,
            params_str.join(", "),
            ret_ty_str,
            body_str
        ))
    }

    /// Emit builtin function call
    fn emit_builtin_call(
        &self,
        builtin: &AotBuiltinOp,
        raw_args: &[AotExpr],
        args: &[String],
        arg_types: &[StaticType],
        return_ty: &StaticType,
    ) -> AotResult<String> {
        match builtin {
            // Basic math functions - use Rust's f64 methods where Julia's
            // real-domain behavior matches Rust.
            AotBuiltinOp::Sqrt => Ok(format!(
                "{{ let _sjulia_sqrt_input = {}; let _sjulia_sqrt_arg = _sjulia_sqrt_input as f64; if _sjulia_sqrt_arg < 0.0_f64 {{ throw(RuntimeError::domain_error(\"sqrt was called with a negative real argument but will only return a complex result if called with a complex argument. Try sqrt(Complex(x)).\")) }} else {{ _sjulia_sqrt_arg.sqrt() }} }}",
                args[0]
            )),
            AotBuiltinOp::Sin => Ok(format!("{}.sin()", args[0])),
            AotBuiltinOp::Cos => Ok(format!("{}.cos()", args[0])),
            AotBuiltinOp::Tan => Ok(format!("{}.tan()", args[0])),
            AotBuiltinOp::Asin => Ok(format!("{}.asin()", args[0])),
            AotBuiltinOp::Acos => Ok(format!("{}.acos()", args[0])),
            AotBuiltinOp::Atan => Ok(format!("{}.atan()", args[0])),
            AotBuiltinOp::Atan2 => Ok(format!("{}.atan2({})", args[0], args[1])),
            AotBuiltinOp::Exp => Ok(format!("{}.exp()", args[0])),
            AotBuiltinOp::Log => Ok(format!(
                "{{ let _sjulia_log_input = {}; let _sjulia_log_arg = _sjulia_log_input as f64; if _sjulia_log_arg < 0.0_f64 {{ throw(RuntimeError::domain_error(\"log was called with a negative real argument but will only return a complex result if called with a complex argument. Try log(Complex(x)).\")) }} else {{ _sjulia_log_arg.ln() }} }}",
                args[0]
            )),
            AotBuiltinOp::Abs => match arg_types.first() {
                Some(ty) if ty.is_signed() => Ok(format!("{}.wrapping_abs()", args[0])),
                Some(ty) if ty.is_unsigned() => Ok(args[0].clone()),
                _ => Ok(format!("{}.abs()", args[0])),
            },
            AotBuiltinOp::Floor => Ok(format!("{}.floor()", args[0])),
            AotBuiltinOp::Ceil => Ok(format!("{}.ceil()", args[0])),
            // Julia's default RoundNearest is round-half-to-even (banker's).
            AotBuiltinOp::Round => Ok(format!("{}.round_ties_even()", args[0])),
            AotBuiltinOp::Trunc => Ok(format!("{}.trunc()", args[0])),
            AotBuiltinOp::Min => {
                if args.len() == 2 {
                    Ok(format!("{}.min({})", args[0], args[1]))
                } else {
                    Ok(format!("min({})", args.join(", ")))
                }
            }
            AotBuiltinOp::Max => {
                if args.len() == 2 {
                    Ok(format!("{}.max({})", args[0], args[1]))
                } else {
                    Ok(format!("max({})", args.join(", ")))
                }
            }
            AotBuiltinOp::Clamp => {
                if args.len() == 3 {
                    Ok(format!("{}.clamp({}, {})", args[0], args[1], args[2]))
                } else {
                    Ok(format!("/* clamp: expected 3 args, got {} */", args.len()))
                }
            }
            AotBuiltinOp::Sign => Ok(format!("{}.signum()", args[0])),
            AotBuiltinOp::Signbit => Ok(format!("{}.is_sign_negative()", args[0])),
            AotBuiltinOp::Copysign => Ok(format!("{}.copysign({})", args[0], args[1])),
            // Integer math operations
            AotBuiltinOp::Div => {
                if args.len() == 2 && arg_types.iter().take(2).all(|ty| ty.is_integer()) {
                    Ok(Self::emit_checked_truncating_int_div(
                        &args[0],
                        &args[1],
                        &arg_types[0],
                        &arg_types[1],
                        return_ty,
                    ))
                } else {
                    Ok(format!("{} / {}", args[0], args[1]))
                }
            }
            AotBuiltinOp::Mod => {
                if args.len() == 2 && arg_types.iter().take(2).all(|ty| ty.is_integer()) {
                    Ok(Self::emit_checked_int_mod(
                        &args[0],
                        &args[1],
                        &arg_types[0],
                        &arg_types[1],
                        return_ty,
                    ))
                } else {
                    Ok(format!("{}.rem_euclid({})", args[0], args[1]))
                }
            }
            AotBuiltinOp::Rem => {
                if args.len() == 2 && arg_types.iter().take(2).all(|ty| ty.is_integer()) {
                    Ok(Self::emit_checked_int_rem(
                        &args[0],
                        &args[1],
                        &arg_types[0],
                        &arg_types[1],
                        return_ty,
                    ))
                } else {
                    Ok(format!("{} % {}", args[0], args[1]))
                }
            }
            AotBuiltinOp::Fld => {
                if args.len() == 2 && arg_types.iter().take(2).all(|ty| ty.is_integer()) {
                    Ok(Self::emit_checked_int_fld(
                        &args[0],
                        &args[1],
                        &arg_types[0],
                        &arg_types[1],
                        return_ty,
                    ))
                } else {
                    Ok(format!("({} / {}).floor()", args[0], args[1]))
                }
            }
            AotBuiltinOp::Cld => {
                if args.len() == 2 && arg_types.iter().take(2).all(|ty| ty.is_integer()) {
                    Ok(Self::emit_checked_int_cld(
                        &args[0],
                        &args[1],
                        &arg_types[0],
                        &arg_types[1],
                        return_ty,
                    ))
                } else {
                    Ok(format!("({} / {}).ceil()", args[0], args[1]))
                }
            }
            // Note: gcd, lcm removed - now Pure Julia (base/intfuncs.jl)

            // Special value checks
            AotBuiltinOp::Isnan => Ok(format!("{}.is_nan()", args[0])),
            AotBuiltinOp::Isinf => Ok(format!("{}.is_infinite()", args[0])),
            AotBuiltinOp::Isfinite => Ok(format!("{}.is_finite()", args[0])),

            // Array operations
            AotBuiltinOp::Length => Self::emit_array_length(args, arg_types),
            AotBuiltinOp::Size => Self::emit_array_size(args, arg_types),
            AotBuiltinOp::Ndims => Ok(Self::emit_array_ndims(arg_types)),
            AotBuiltinOp::Push if matches!(arg_types.first(), Some(StaticType::Set { .. })) => {
                Ok(format!("{{ let _ = {}.insert({}); {}.clone() }}", args[0], args[1], args[0]))
            }
            AotBuiltinOp::Push => Ok(format!("{}.push({})", args[0], args[1])),
            AotBuiltinOp::Pop => Ok(format!(
                "{}.pop().expect(\"pop! from empty collection\")",
                args[0]
            )),
            AotBuiltinOp::PushFirst => Ok(format!("{}.insert(0, {})", args[0], args[1])),
            AotBuiltinOp::PopFirst => Ok(format!(
                "{{ if {}.is_empty() {{ panic!(\"popfirst! from empty collection\") }} else {{ {}.remove(0) }} }}",
                args[0], args[0]
            )),
            // insert!(arr, i, x) -> arr.insert((i - 1) as usize, x)
            // Julia uses 1-based indexing, Rust uses 0-based
            AotBuiltinOp::Insert => {
                if args.len() >= 3 {
                    Ok(format!(
                        "{}.insert(({} - 1) as usize, {})",
                        args[0], args[1], args[2]
                    ))
                } else {
                    Ok("/* insert!: insufficient args */".to_string())
                }
            }
            // deleteat!(arr, i) -> arr.remove((i - 1) as usize)
            AotBuiltinOp::DeleteAt => {
                if args.len() >= 2 {
                    Ok(format!("{}.remove(({} - 1) as usize)", args[0], args[1]))
                } else {
                    Ok("/* deleteat!: insufficient args */".to_string())
                }
            }
            // append!(arr, other) -> arr.extend(other.iter().cloned())
            AotBuiltinOp::Append => {
                if args.len() >= 2 {
                    Ok(format!("{}.extend({}.iter().cloned())", args[0], args[1]))
                } else {
                    Ok("/* append!: insufficient args */".to_string())
                }
            }
            // first(arr) -> arr[0].clone()
            AotBuiltinOp::First => Ok(format!("{}[0].clone()", args[0])),
            // last(arr) -> arr[arr.len() - 1].clone()
            AotBuiltinOp::Last => Ok(format!("{}[{}.len() - 1].clone()", args[0], args[0])),
            AotBuiltinOp::TupleFirst => Self::emit_tuple_edge_access(args, arg_types, true),
            AotBuiltinOp::TupleLast => Self::emit_tuple_edge_access(args, arg_types, false),
            // isempty(arr) -> arr.is_empty()
            AotBuiltinOp::IsEmpty => Ok(format!("{}.is_empty()", args[0])),
            AotBuiltinOp::In => {
                if args.len() >= 2 {
                    Ok(format!("{}.contains(&{})", args[1], args[0]))
                } else {
                    Ok("/* in: insufficient args */".to_string())
                }
            }
            AotBuiltinOp::Dict => self.emit_dict_constructor(raw_args, return_ty),
            AotBuiltinOp::HasKey => {
                if args.len() >= 2 {
                    Ok(format!("{}.contains_key(&{})", args[0], args[1]))
                } else {
                    Ok("/* haskey: insufficient args */".to_string())
                }
            }
            AotBuiltinOp::DictGet => {
                if args.len() >= 3 && matches!(arg_types.first(), Some(StaticType::Dict { .. })) {
                    Ok(format!("{}.get(&{}).cloned().unwrap_or({})", args[0], args[1], args[2]))
                } else {
                    Err(AotError::UnsupportedInstruction(
                        UnsupportedInstructionDiagnostic::new(
                            "AoT codegen supports get(dict, key, default) only for statically typed Dict inputs (Issue #7034)",
                        )
                        .with_workaround(
                            "call get with a concrete Dict, key, and default value, or run this code on the VM",
                        ),
                    ))
                }
            }
            // collect(iter) -> iter.collect::<Vec<_>>()
            AotBuiltinOp::Collect => Ok(format!(
                "{}.collect::<Vec<_>>()",
                self.emit_owned_iter_expr(&raw_args[0])?
            )),
            // zeros(n), zeros(m, n), zeros(m, n, ...)
            AotBuiltinOp::Zeros => {
                let fill = Self::array_fill_literal(return_ty, false)?;
                if args.len() == 1 {
                    // 1D: zeros(n)
                    Ok(format!("vec![{}; {} as usize]", fill, args[0]))
                } else if args.len() == 2 {
                    // 2D: zeros(rows, cols) -> Vec<Vec<T>>
                    Ok(format!(
                        "(0..{} as usize).map(|_| vec![{}; {} as usize]).collect::<Vec<_>>()",
                        args[0], fill, args[1]
                    ))
                } else if !args.is_empty() {
                    Ok(Self::emit_nested_fill_vec(&fill, args))
                } else {
                    Ok("vec![]".to_string())
                }
            }
            // ones(n), ones(m, n), ones(m, n, ...)
            AotBuiltinOp::Ones => {
                let fill = Self::array_fill_literal(return_ty, true)?;
                if args.len() == 1 {
                    // 1D: ones(n)
                    Ok(format!("vec![{}; {} as usize]", fill, args[0]))
                } else if args.len() == 2 {
                    // 2D: ones(rows, cols) -> Vec<Vec<T>>
                    Ok(format!(
                        "(0..{} as usize).map(|_| vec![{}; {} as usize]).collect::<Vec<_>>()",
                        args[0], fill, args[1]
                    ))
                } else if !args.is_empty() {
                    Ok(Self::emit_nested_fill_vec(&fill, args))
                } else {
                    Ok("vec![]".to_string())
                }
            }
            // Note: Fill removed — now Pure Julia (Issue #2640)
            AotBuiltinOp::Reshape => Ok(format!("{} /* reshape */", args[0])),
            AotBuiltinOp::Sum => {
                let sum_ty = self.type_to_rust(return_ty);
                if args.len() == 1 {
                    Ok(format!(
                        "{}.sum::<{}>()",
                        self.emit_owned_iter_expr(&raw_args[0])?,
                        sum_ty
                    ))
                } else if args.len() >= 2 {
                    let elem_ty = Self::hof_array_element_type(&raw_args[1]);
                    let function = self.hof_function_expr(&raw_args[0], &[elem_ty])?;
                    let mapped = Self::emit_hof_unary_call(&function, "x");
                    Ok(format!(
                        "{}.map(|x| {}).sum::<{}>()",
                        self.emit_owned_iter_expr(&raw_args[1])?,
                        mapped,
                        sum_ty
                    ))
                } else {
                    Ok("/* sum: insufficient args */".to_string())
                }
            }

            // Higher-order functions
            // Note: These expect the function as the first argument, array as second
            // For anonymous functions (closures), the closure syntax is passed directly
            AotBuiltinOp::Map => {
                if args.len() >= 2 {
                    // map(f, arr) clones elements so non-Copy arrays (String/struct) work.
                    let elem_ty = Self::hof_array_element_type(&raw_args[1]);
                    let function = self.hof_function_expr(&raw_args[0], &[elem_ty])?;
                    let mapped = Self::emit_hof_unary_call(&function, "x");
                    Ok(format!(
                        "{}.map(|x| {}).collect::<Vec<_>>()",
                        self.emit_owned_iter_expr(&raw_args[1])?,
                        mapped
                    ))
                } else {
                    Ok("/* map: insufficient args */".to_string())
                }
            }
            AotBuiltinOp::Filter => {
                if args.len() >= 2 {
                    // filter(f, arr) passes cloned values to predicates and clones retained
                    // elements into the result, avoiding Copy-only destructuring.
                    let elem_ty = Self::hof_array_element_type(&raw_args[1]);
                    let function = self.hof_function_expr(&raw_args[0], &[elem_ty])?;
                    let predicate = Self::emit_hof_unary_call(&function, "(*x).clone()");
                    Ok(format!(
                        "{}.filter(|x| {}).collect::<Vec<_>>()",
                        self.emit_owned_iter_expr(&raw_args[1])?,
                        predicate
                    ))
                } else {
                    Ok("/* filter: insufficient args */".to_string())
                }
            }
            AotBuiltinOp::Reduce => {
                if args.len() >= 2 {
                    // reduce(f, arr) -> arr.iter().cloned().reduce(f)
                    let elem_ty = Self::hof_array_element_type(&raw_args[1]);
                    let function =
                        self.hof_function_expr(&raw_args[0], &[elem_ty.clone(), elem_ty])?;
                    let reduced = Self::emit_hof_binary_call(&function, "a", "b");
                    Ok(format!(
                        "{}.reduce(|a, b| {}).unwrap_or_default()",
                        self.emit_owned_iter_expr(&raw_args[1])?,
                        reduced
                    ))
                } else {
                    Ok("/* reduce: insufficient args */".to_string())
                }
            }
            AotBuiltinOp::MapReduce => {
                if args.len() >= 3 {
                    let elem_ty = Self::hof_array_element_type(&raw_args[2]);
                    let map_function = self.hof_function_expr(&raw_args[0], &[elem_ty])?;
                    let mapped_ty = match return_ty {
                        StaticType::Any => StaticType::Any,
                        other => other.clone(),
                    };
                    let op_function =
                        self.hof_function_expr(&raw_args[1], &[mapped_ty.clone(), mapped_ty])?;
                    let mapped = Self::emit_hof_unary_call(&map_function, "x");
                    let reduced = Self::emit_hof_binary_call(&op_function, "a", "b");
                    Ok(format!(
                        "{}.map(|x| {}).reduce(|a, b| {}).unwrap_or_default()",
                        self.emit_owned_iter_expr(&raw_args[2])?,
                        mapped,
                        reduced
                    ))
                } else {
                    Ok("/* mapreduce: insufficient args */".to_string())
                }
            }
            AotBuiltinOp::ForEach => {
                if args.len() >= 2 {
                    // foreach(f, arr) -> arr.iter().for_each(f) for closures
                    // or arr.iter().for_each(|&x| { f(x); }) for named functions
                    if Self::is_closure_literal(&args[0]) {
                        Ok(format!("{}.iter().copied().for_each({})", args[1], args[0]))
                    } else {
                        Ok(format!(
                            "{}.iter().for_each(|&x| {{ {}(x); }})",
                            args[1], args[0]
                        ))
                    }
                } else {
                    Ok("/* foreach: insufficient args */".to_string())
                }
            }
            AotBuiltinOp::Any => {
                if args.len() >= 2 {
                    // any(f, arr) -> arr.iter().any(f) for closures
                    // or arr.iter().any(|&x| f(x)) for named functions
                    if Self::is_closure_literal(&args[0]) {
                        Ok(format!("{}.iter().copied().any({})", args[1], args[0]))
                    } else {
                        Ok(format!("{}.iter().any(|&x| {}(x))", args[1], args[0]))
                    }
                } else {
                    Ok("/* any: insufficient args */".to_string())
                }
            }
            AotBuiltinOp::All => {
                if args.len() >= 2 {
                    // all(f, arr) -> arr.iter().all(f) for closures
                    // or arr.iter().all(|&x| f(x)) for named functions
                    if Self::is_closure_literal(&args[0]) {
                        Ok(format!("{}.iter().copied().all({})", args[1], args[0]))
                    } else {
                        Ok(format!("{}.iter().all(|&x| {}(x))", args[1], args[0]))
                    }
                } else {
                    Ok("/* all: insufficient args */".to_string())
                }
            }

            // String operations
            AotBuiltinOp::StringLength => Ok(format!("{}.len() as i64", args[0])),
            AotBuiltinOp::Uppercase => Ok(format!("{}.to_uppercase()", args[0])),
            AotBuiltinOp::Lowercase => Ok(format!("{}.to_lowercase()", args[0])),
            // Julia `occursin(needle, haystack)` → `haystack.contains(needle)`;
            // `startswith` / `endswith` keep argument order. A `String` pattern
            // uses `.as_str()` and a `Char` pattern is passed directly so both
            // satisfy Rust's `Pattern` (Issue #7058).
            AotBuiltinOp::Occursin => Ok(format!(
                "({}).contains({})",
                args[1],
                Self::string_pattern_arg(&args[0], arg_types.first())
            )),
            AotBuiltinOp::StartsWith => Ok(format!(
                "({}).starts_with({})",
                args[0],
                Self::string_pattern_arg(&args[1], arg_types.get(1))
            )),
            AotBuiltinOp::EndsWith => Ok(format!(
                "({}).ends_with({})",
                args[0],
                Self::string_pattern_arg(&args[1], arg_types.get(1))
            )),

            // I/O operations
            // Generate format string with one {} for each argument
            AotBuiltinOp::Println => {
                if args.is_empty() {
                    return Ok("println!()".to_string());
                }
                let display_args = Self::julia_display_args(args, arg_types);
                let format_specifiers: String = display_args
                    .iter()
                    .map(|_| "{}")
                    .collect::<Vec<_>>()
                    .join("");
                Ok(format!(
                    "println!(\"{}\", {})",
                    format_specifiers,
                    display_args.join(", ")
                ))
            }
            AotBuiltinOp::Print => {
                if args.is_empty() {
                    return Ok("print!(\"\")".to_string());
                }
                let display_args = Self::julia_display_args(args, arg_types);
                let format_specifiers: String = display_args
                    .iter()
                    .map(|_| "{}")
                    .collect::<Vec<_>>()
                    .join("");
                Ok(format!(
                    "print!(\"{}\", {})",
                    format_specifiers,
                    display_args.join(", ")
                ))
            }
            AotBuiltinOp::TimeNs => Ok(
                "std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect(\"time went backwards\").as_nanos() as i64".to_string()
            ),

            // Type operations
            AotBuiltinOp::TypeOf => {
                let Some(arg) = args.first() else {
                    return Err(AotError::CodegenError(
                        "AoT typeof codegen requires one argument".to_string(),
                    ));
                };
                let Some(arg_ty) = arg_types.first() else {
                    return Err(AotError::CodegenError(
                        "AoT typeof codegen requires an argument type".to_string(),
                    ));
                };
                if AotAbiValue::from_static_type(arg_ty).needs_runtime_value() {
                    Ok(format!("Value::DataType({}.type_name().to_string())", arg))
                } else {
                    Ok(format!(
                        "Value::DataType({:?}.to_string())",
                        arg_ty.julia_type_name()
                    ))
                }
            }
            AotBuiltinOp::Isa => Ok("/* isa check */ true".to_string()),

            AotBuiltinOp::Rand | AotBuiltinOp::Randn => {
                let sample = match builtin {
                    AotBuiltinOp::Rand => "__sjulia_aot_rand()",
                    AotBuiltinOp::Randn => "__sjulia_aot_randn()",
                    _ => unreachable!(),
                };
                if args.is_empty() {
                    Ok(sample.to_string())
                } else {
                    Ok(Self::emit_nested_random_vec(sample, args))
                }
            }

            // Type conversion intrinsics
            // sitofp(Float64, x) -> x as f64
            AotBuiltinOp::Sitofp => {
                // Second argument is the value to convert (first is the type)
                if args.len() >= 2 {
                    Ok(format!("({} as f64)", args[1]))
                } else if args.len() == 1 {
                    Ok(format!("({} as f64)", args[0]))
                } else {
                    Ok("/* sitofp: missing args */ 0.0_f64".to_string())
                }
            }
            // fptosi: checked float→integer with Julia InexactError semantics
            // (Issue #7038), sharing the Convert round-trip helper.
            AotBuiltinOp::Fptosi => {
                let source = arg_types.first().cloned().unwrap_or(StaticType::F64);
                if Self::is_float32_or_float64(&source) && Self::integer_layout(return_ty).is_some() {
                    Ok(self.emit_checked_float_to_int(&args[0], &source, return_ty))
                } else {
                    Err(AotError::CodegenError(format!(
                        "AoT codegen cannot lower fptosi from {} to {}; expected float→integer (Issue #7038)",
                        source, return_ty
                    )))
                }
            }

            // Error handling: throw(e) maps to the runtime's diverging `aot_throw`
            // rather than an inline `panic!`, keeping generated code free of raw
            // `panic!` (Issue #3406 / #5658).
            AotBuiltinOp::Throw => {
                if args.len() == 1 {
                    Ok(format!(
                        "subset_julia_vm_runtime::error::aot_throw({})",
                        args[0]
                    ))
                } else {
                    Ok("subset_julia_vm_runtime::error::aot_throw(\"error\")".to_string())
                }
            }

            // Complex number operations (Issue #3410)
            AotBuiltinOp::Abs2 => Ok(format!("abs2_complex({})", args[0])),
            AotBuiltinOp::Real => Ok(format!("real_complex({})", args[0])),
            AotBuiltinOp::Imag => Ok(format!("imag_complex({})", args[0])),

            // Transpose (Issue #3410)
            AotBuiltinOp::Adjoint => Ok(format!("adjoint_vec({})", args[0])),

            // Linspace (Issue #3413)
            AotBuiltinOp::Linspace => {
                if args.len() >= 3 {
                    Ok(format!("linspace({}, {}, {})", args[0], args[1], args[2]))
                } else {
                    Ok("vec![]".to_string())
                }
            }

            // String concatenation: string(a, b, ...) -> format!("{}{}", a, b, ...) (Issue #3405)
            AotBuiltinOp::StringConcat => {
                if args.is_empty() {
                    Ok("String::new()".to_string())
                } else if args.len() == 1 {
                    Ok(format!(
                        "format!(\"{{}}\", {})",
                        Self::julia_display_arg(&args[0], &arg_types[0])
                    ))
                } else {
                    let display_args = Self::julia_display_args(args, arg_types);
                    let format_specifiers: String = display_args
                        .iter()
                        .map(|_| "{}")
                        .collect::<Vec<_>>()
                        .join("");
                    Ok(format!(
                        "format!(\"{}\", {})",
                        format_specifiers,
                        display_args.join(", ")
                    ))
                }
            }
        }
    }

    fn julia_display_args(args: &[String], arg_types: &[StaticType]) -> Vec<String> {
        args.iter()
            .zip(arg_types.iter())
            .map(|(arg, ty)| Self::julia_display_arg(arg, ty))
            .collect()
    }

    fn julia_display_arg(arg: &str, ty: &StaticType) -> String {
        match ty {
            StaticType::F64 => format!("__sjulia_format_float64({})", arg),
            StaticType::F32 => format!("__sjulia_format_float32({})", arg),
            StaticType::Array { .. } | StaticType::Tuple(_) => Self::julia_show_expr(arg, ty),
            _ => arg.to_string(),
        }
    }

    fn julia_show_expr(arg: &str, ty: &StaticType) -> String {
        match ty {
            StaticType::F64 => format!("__sjulia_format_float64({})", arg),
            StaticType::F32 => format!("__sjulia_format_float32({})", arg),
            StaticType::Str => format!("format!(\"\\\"{{}}\\\"\", {})", arg),
            StaticType::Char => format!("format!(\"'{{}}'\", {})", arg),
            StaticType::Array { element, ndims } => {
                Self::julia_show_array_expr(arg, element, *ndims)
            }
            StaticType::Tuple(elements) => Self::julia_show_tuple_expr(arg, elements),
            _ => format!("format!(\"{{}}\", {})", arg),
        }
    }

    fn julia_show_ref_expr(arg: &str, ty: &StaticType) -> String {
        match ty {
            StaticType::F64 => format!("__sjulia_format_float64(*{})", arg),
            StaticType::F32 => format!("__sjulia_format_float32(*{})", arg),
            StaticType::Str => format!("format!(\"\\\"{{}}\\\"\", {})", arg),
            StaticType::Char => format!("format!(\"'{{}}'\", {})", arg),
            StaticType::Array { element, ndims } => {
                Self::julia_show_array_expr(arg, element, *ndims)
            }
            StaticType::Tuple(elements) => Self::julia_show_tuple_ref_expr(arg, elements),
            _ => format!("format!(\"{{}}\", *{})", arg),
        }
    }

    fn julia_show_array_expr(arg: &str, element: &StaticType, ndims: Option<usize>) -> String {
        if ndims == Some(2) {
            let row = "__sjulia_row";
            let item = "__sjulia_item";
            let item_display = Self::julia_show_ref_expr(item, element);
            return format!(
                "format!(\"[{{}}]\", ({arg}).iter().map(|{row}| {row}.iter().map(|{item}| {item_display}).collect::<Vec<_>>().join(\" \")).collect::<Vec<_>>().join(\"; \"))"
            );
        }

        let item = "__sjulia_item";
        let item_display = Self::julia_show_ref_expr(item, element);
        format!(
            "format!(\"[{{}}]\", ({arg}).iter().map(|{item}| {item_display}).collect::<Vec<_>>().join(\", \"))"
        )
    }

    fn julia_show_tuple_expr(arg: &str, elements: &[StaticType]) -> String {
        let tuple = "__sjulia_tuple";
        let body = Self::julia_show_tuple_body(tuple, elements);
        format!("{{ let {tuple} = &{arg}; {body} }}")
    }

    fn julia_show_tuple_ref_expr(arg: &str, elements: &[StaticType]) -> String {
        Self::julia_show_tuple_body(arg, elements)
    }

    fn julia_show_tuple_body(tuple: &str, elements: &[StaticType]) -> String {
        let rendered = elements
            .iter()
            .enumerate()
            .map(|(idx, ty)| {
                let field = format!("{tuple}.{idx}");
                Self::julia_show_ref_expr(&format!("&{field}"), ty)
            })
            .collect::<Vec<_>>();
        let comma = if elements.len() == 1 { "," } else { "" };
        if rendered.is_empty() {
            "\"()\".to_string()".to_string()
        } else {
            format!(
                "format!(\"({}{comma})\", {})",
                std::iter::repeat_n("{}", rendered.len())
                    .collect::<Vec<_>>()
                    .join(", "),
                rendered.join(", ")
            )
        }
    }

    fn emit_array_length(args: &[String], arg_types: &[StaticType]) -> AotResult<String> {
        let Some(array) = args.first() else {
            return Err(AotError::CodegenError(
                "AoT length codegen requires an array argument".to_string(),
            ));
        };
        match Self::array_rank(arg_types) {
            Some(1) | None => Ok(format!("{}.len() as i64", array)),
            Some(2) => Ok(format!(
                "{{ let _sjulia_arr = &{}; (_sjulia_arr.len() as i64) * if _sjulia_arr.is_empty() {{ 0i64 }} else {{ _sjulia_arr[0].len() as i64 }} }}",
                array
            )),
            Some(rank) => Ok(format!(
                "{{ let _sjulia_arr_0 = &{}; {} ({}) as i64 }}",
                array,
                Self::emit_array_dim_bindings(rank),
                Self::array_length_product_expr(rank)
            )),
        }
    }

    fn emit_array_size(args: &[String], arg_types: &[StaticType]) -> AotResult<String> {
        let Some(array) = args.first() else {
            return Err(AotError::CodegenError(
                "AoT size codegen requires an array argument".to_string(),
            ));
        };
        match (args.len(), Self::array_rank(arg_types)) {
            (1, Some(1) | None) => Ok(format!("({}.len() as i64,)", array)),
            (1, Some(2)) => Ok(format!(
                "{{ let _sjulia_arr = &{}; (_sjulia_arr.len() as i64, if _sjulia_arr.is_empty() {{ 0i64 }} else {{ _sjulia_arr[0].len() as i64 }}) }}",
                array
            )),
            (1, Some(rank)) => Ok(format!(
                "{{ let _sjulia_arr_0 = &{}; {} ({}) }}",
                array,
                Self::emit_array_dim_bindings(rank),
                (0..rank)
                    .map(|dim| format!("_sjulia_dim_{dim} as i64"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            (_, Some(1) | None) => Ok(format!(
                "{{ let _sjulia_dim = {}; if _sjulia_dim < 1 {{ subset_julia_vm_runtime::error::aot_throw(\"Dimension out of range\"); }} else if _sjulia_dim == 1 {{ {}.len() as i64 }} else {{ 1i64 }} }}",
                args[1], array
            )),
            (_, Some(2)) => Ok(format!(
                "{{ let _sjulia_arr = &{}; let _sjulia_dim = {}; if _sjulia_dim < 1 {{ subset_julia_vm_runtime::error::aot_throw(\"Dimension out of range\"); }} else if _sjulia_dim == 1 {{ _sjulia_arr.len() as i64 }} else if _sjulia_dim == 2 {{ if _sjulia_arr.is_empty() {{ 0i64 }} else {{ _sjulia_arr[0].len() as i64 }} }} else {{ 1i64 }} }}",
                array, args[1]
            )),
            (_, Some(rank)) => Ok(format!(
                "{{ let _sjulia_arr_0 = &{}; {} let _sjulia_dim = {}; if _sjulia_dim < 1 {{ subset_julia_vm_runtime::error::aot_throw(\"Dimension out of range\"); }} {} else {{ 1i64 }} }}",
                array,
                Self::emit_array_dim_bindings(rank),
                args[1],
                (0..rank)
                    .map(|dim| format!(
                        "else if _sjulia_dim == {}i64 {{ _sjulia_dim_{} as i64 }}",
                        dim + 1,
                        dim
                    ))
                    .collect::<Vec<_>>()
                    .join(" ")
            )),
        }
    }

    fn emit_array_ndims(arg_types: &[StaticType]) -> String {
        format!("{}i64", Self::array_rank(arg_types).unwrap_or(1))
    }

    fn array_rank(arg_types: &[StaticType]) -> Option<usize> {
        match arg_types.first() {
            Some(StaticType::Array { ndims, .. }) => *ndims,
            _ => None,
        }
    }

    fn emit_array_dim_bindings(rank: usize) -> String {
        let mut code = String::new();
        for dim in 0..rank {
            let parent = Self::nested_zero_index_expr("_sjulia_arr_0", dim);
            let empty_guard = (0..dim)
                .map(|prev| format!("_sjulia_dim_{prev} == 0usize"))
                .collect::<Vec<_>>()
                .join(" || ");
            if dim == 0 {
                code.push_str("let _sjulia_dim_0 = _sjulia_arr_0.len();");
            } else if empty_guard.is_empty() {
                code.push_str(&format!(" let _sjulia_dim_{dim} = {parent}.len();"));
            } else {
                code.push_str(&format!(
                    " let _sjulia_dim_{dim} = if {empty_guard} {{ 0usize }} else {{ {parent}.len() }};"
                ));
            }
        }
        code
    }

    fn array_length_product_expr(rank: usize) -> String {
        (0..rank)
            .map(|dim| format!("_sjulia_dim_{dim}"))
            .collect::<Vec<_>>()
            .join(" * ")
    }

    fn nested_zero_index_expr(root: &str, depth: usize) -> String {
        (0..depth).fold(root.to_string(), |acc, _| format!("{acc}[0]"))
    }

    fn emit_nested_fill_vec(fill: &str, dims: &[String]) -> String {
        let mut iter = dims.iter().rev();
        let Some(last) = iter.next() else {
            return "vec![]".to_string();
        };
        let mut result = format!("vec![{}; {} as usize]", fill, last);
        for dim in iter {
            result = format!("vec![{}; {} as usize]", result, dim);
        }
        result
    }

    fn emit_nested_random_vec(sample: &str, dims: &[String]) -> String {
        let mut result = sample.to_string();
        for dim in dims.iter().rev() {
            result = format!(
                "(0..{} as usize).map(|_| {}).collect::<Vec<_>>()",
                dim, result
            );
        }
        result
    }

    fn array_fill_literal(return_ty: &StaticType, one: bool) -> AotResult<String> {
        let elem_ty = match return_ty {
            StaticType::Array { element, .. } => element.as_ref(),
            _ => &StaticType::F64,
        };

        let literal = match elem_ty {
            StaticType::I8 => {
                if one {
                    "1i8"
                } else {
                    "0i8"
                }
            }
            StaticType::I16 => {
                if one {
                    "1i16"
                } else {
                    "0i16"
                }
            }
            StaticType::I32 => {
                if one {
                    "1i32"
                } else {
                    "0i32"
                }
            }
            StaticType::I64 => {
                if one {
                    "1i64"
                } else {
                    "0i64"
                }
            }
            StaticType::I128 => {
                if one {
                    "1i128"
                } else {
                    "0i128"
                }
            }
            StaticType::U8 => {
                if one {
                    "1u8"
                } else {
                    "0u8"
                }
            }
            StaticType::U16 => {
                if one {
                    "1u16"
                } else {
                    "0u16"
                }
            }
            StaticType::U32 => {
                if one {
                    "1u32"
                } else {
                    "0u32"
                }
            }
            StaticType::U64 => {
                if one {
                    "1u64"
                } else {
                    "0u64"
                }
            }
            StaticType::U128 => {
                if one {
                    "1u128"
                } else {
                    "0u128"
                }
            }
            StaticType::F32 | StaticType::F16 => {
                if one {
                    "1.0_f32"
                } else {
                    "0.0_f32"
                }
            }
            StaticType::F64 => {
                if one {
                    "1.0_f64"
                } else {
                    "0.0_f64"
                }
            }
            StaticType::Bool => {
                if one {
                    "true"
                } else {
                    "false"
                }
            }
            other => {
                return Err(AotError::CodegenError(format!(
                    "zeros/ones AoT codegen does not support element type {:?}",
                    other
                )))
            }
        };

        Ok(literal.to_string())
    }

    fn emit_tuple_edge_access(
        args: &[String],
        arg_types: &[StaticType],
        first: bool,
    ) -> AotResult<String> {
        let Some(tuple_expr) = args.first() else {
            return Ok("/* tuple access: insufficient args */".to_string());
        };

        match arg_types.first() {
            Some(StaticType::Tuple(elements)) if !elements.is_empty() => {
                let index = if first { 0 } else { elements.len() - 1 };
                Ok(format!("{}.{}", tuple_expr, index))
            }
            Some(StaticType::Tuple(_)) => Err(AotError::CodegenError(
                "AoT codegen does not support first/last on empty tuple".to_string(),
            )),
            other => Err(AotError::CodegenError(format!(
                "AoT tuple first/last expected tuple argument, got {:?}",
                other
            ))),
        }
    }
}

/// Build a nested `vec![...]` string from column-major flat elements.
///
/// Julia stores multi-dimensional arrays in column-major order:
///   element[i0, i1, ..., i_{n-1}] = flat[i0 + i1*s0 + i2*s0*s1 + ...]
///
/// This function recursively builds nested vecs so that
///   `arr[i0][i1]...[i_{n-1}]` indexes correctly in Rust.
fn build_nested_vec_colmajor(
    elems: &[String],
    shape: &[usize],
    dim: usize,
    offset: usize,
) -> String {
    let stride: usize = shape[..dim].iter().product();
    if dim == shape.len() - 1 {
        // Innermost dimension: collect scalar elements
        let items: Vec<_> = (0..shape[dim])
            .filter_map(|i| {
                let flat_idx = offset + i * stride;
                elems.get(flat_idx).cloned()
            })
            .collect();
        format!("vec![{}]", items.join(", "))
    } else {
        // Recurse: each slot at this dimension produces a sub-vec
        let items: Vec<_> = (0..shape[dim])
            .map(|i| {
                let sub_offset = offset + i * stride;
                build_nested_vec_colmajor(elems, shape, dim + 1, sub_offset)
            })
            .collect();
        format!("vec![{}]", items.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_nested_vec_1d() {
        let elems: Vec<String> = vec!["1".into(), "2".into(), "3".into()];
        let result = build_nested_vec_colmajor(&elems, &[3], 0, 0);
        assert_eq!(result, "vec![1, 2, 3]");
    }

    #[test]
    fn test_build_nested_vec_2d_colmajor() {
        // Julia: [1 3; 2 4] -> shape [2, 2], column-major flat: [1, 2, 3, 4]
        // Expected Rust: vec![vec![1, 3], vec![2, 4]]
        //   arr[0][0]=1, arr[0][1]=3, arr[1][0]=2, arr[1][1]=4
        let elems: Vec<String> = vec!["1".into(), "2".into(), "3".into(), "4".into()];
        let result = build_nested_vec_colmajor(&elems, &[2, 2], 0, 0);
        assert_eq!(result, "vec![vec![1, 3], vec![2, 4]]");
    }

    #[test]
    fn test_build_nested_vec_2d_nonsquare() {
        // Julia: [1 3 5; 2 4 6] -> shape [2, 3], column-major flat: [1, 2, 3, 4, 5, 6]
        // Expected Rust: vec![vec![1, 3, 5], vec![2, 4, 6]]
        let elems: Vec<String> = vec![
            "1".into(),
            "2".into(),
            "3".into(),
            "4".into(),
            "5".into(),
            "6".into(),
        ];
        let result = build_nested_vec_colmajor(&elems, &[2, 3], 0, 0);
        assert_eq!(result, "vec![vec![1, 3, 5], vec![2, 4, 6]]");
    }

    #[test]
    fn test_build_nested_vec_3d_colmajor() {
        // 3D array shape [2, 2, 2], column-major flat: [1,2,3,4,5,6,7,8]
        // Julia indexing:
        //   [1,1,1]=1, [2,1,1]=2, [1,2,1]=3, [2,2,1]=4,
        //   [1,1,2]=5, [2,1,2]=6, [1,2,2]=7, [2,2,2]=8
        // Rust arr[i][j][k]:
        //   arr[0][0][0]=1, arr[1][0][0]=2, arr[0][1][0]=3, arr[1][1][0]=4,
        //   arr[0][0][1]=5, arr[1][0][1]=6, arr[0][1][1]=7, arr[1][1][1]=8
        // = vec![vec![vec![1,5], vec![3,7]], vec![vec![2,6], vec![4,8]]]
        let elems: Vec<String> = (1..=8).map(|i| i.to_string()).collect();
        let result = build_nested_vec_colmajor(&elems, &[2, 2, 2], 0, 0);
        assert_eq!(
            result,
            "vec![vec![vec![1, 5], vec![3, 7]], vec![vec![2, 6], vec![4, 8]]]"
        );
    }

    #[test]
    fn test_build_nested_vec_3d_nonsymmetric() {
        // 3D array shape [2, 3, 1], column-major flat: [1,2,3,4,5,6]
        // Only 1 "layer" in dim 2, so this is effectively a 2D matrix laid out as 3D.
        // arr[i][j][0]:
        //   arr[0][0][0]=1, arr[1][0][0]=2
        //   arr[0][1][0]=3, arr[1][1][0]=4
        //   arr[0][2][0]=5, arr[1][2][0]=6
        let elems: Vec<String> = (1..=6).map(|i| i.to_string()).collect();
        let result = build_nested_vec_colmajor(&elems, &[2, 3, 1], 0, 0);
        assert_eq!(
            result,
            "vec![vec![vec![1], vec![3], vec![5]], vec![vec![2], vec![4], vec![6]]]"
        );
    }
}
