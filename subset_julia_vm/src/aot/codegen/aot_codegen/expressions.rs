use super::AotCodeGenerator;
use crate::aot::ir::{AotBinOp, AotBuiltinOp, AotExpr, AotFunction, AotUnaryOp};
use crate::aot::types::StaticType;
use crate::aot::AotResult;

impl AotCodeGenerator {
    // ========== Expression Generation ==========

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

            // Variable
            AotExpr::Var { name, .. } => Ok(name.clone()),

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
                // For dynamic operations, we'd need runtime dispatch
                // For now, generate static code with a comment
                let left_str = self.emit_expr_to_string(left)?;
                let right_str = self.emit_expr_to_string(right)?;
                Ok(format!(
                    "({} {} {}) /* dynamic */",
                    left_str,
                    op.to_rust_op(),
                    right_str
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

                // Resolve static dispatch: if this function has multiple methods,
                // use the mangled name based on argument types
                let resolved_name = if self.needs_dispatch(function) {
                    // Get argument types and compute mangled name
                    let arg_types: Vec<_> = args.iter().map(|a| a.get_type()).collect();
                    self.resolve_dispatch(function, &arg_types)
                } else {
                    AotFunction::sanitize_function_name(function)
                };

                Ok(format!("{}({})", resolved_name, args_str.join(", ")))
            }

            AotExpr::CallDynamic { function, args } => {
                let args_str: Vec<_> = args
                    .iter()
                    .map(|a| self.emit_expr_to_string(a))
                    .collect::<AotResult<_>>()?;
                Ok(format!(
                    "{}({}) /* dynamic */",
                    AotFunction::sanitize_function_name(function),
                    args_str.join(", ")
                ))
            }

            AotExpr::CallBuiltin { builtin, args, .. } => {
                let args_str: Vec<_> = args
                    .iter()
                    .map(|a| self.emit_expr_to_string(a))
                    .collect::<AotResult<_>>()?;
                self.emit_builtin_call(builtin, &args_str)
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
                } else if shape.iter().any(|&d| d == 0) {
                    // Any zero dimension: empty nested vec
                    let inner = "vec![]".to_string();
                    let result = (1..shape.len()).fold(inner, |acc, _| {
                        format!("vec![{}]", acc)
                    });
                    Ok(result)
                } else {
                    // N-dimensional array (2D, 3D, ...): nested Vec
                    // Julia stores column-major: element[i0,i1,...] =
                    //   elements[i0 + i1*shape[0] + i2*shape[0]*shape[1] + ...]
                    // Build nested vec so arr[i0][i1]...[i_{n-1}] indexes correctly
                    Ok(build_nested_vec_colmajor(&elems_str, shape, 0, 0))
                }
            }

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

            // Index (1D or multidimensional, or tuple)
            AotExpr::Index {
                array,
                indices,
                is_tuple,
                ..
            } => {
                let array_str = self.emit_expr_to_string(array)?;
                let _array_ty = array.get_type();

                if indices.is_empty() {
                    // Empty indices - shouldn't happen, but handle gracefully
                    Ok(array_str)
                } else if *is_tuple && indices.len() == 1 {
                    // Tuple indexing: t[1] -> t.0 (Julia 1-indexed to Rust .0, .1, etc.)
                    // The index must be a literal for tuple access in Rust
                    match &indices[0] {
                        AotExpr::LitI64(idx) => {
                            // Convert 1-based Julia index to 0-based Rust tuple field
                            let rust_idx = idx - 1;
                            Ok(format!("{}.{}", array_str, rust_idx))
                        }
                        _ => {
                            // Non-literal index: fall back to array-style (may not compile in Rust)
                            let index_str = self.emit_expr_to_string(&indices[0])?;
                            Ok(format!("{}[({}) as usize - 1]", array_str, index_str))
                        }
                    }
                } else if indices.len() == 1 {
                    // 1D array indexing: arr[i]
                    let index_str = self.emit_expr_to_string(&indices[0])?;
                    // Julia is 1-indexed, Rust is 0-indexed
                    Ok(format!("{}[({}) as usize - 1]", array_str, index_str))
                } else {
                    // Multidimensional indexing: mat[i, j] -> mat[(i-1) as usize][(j-1) as usize]
                    // For row-major Vec<Vec<_>> representation
                    let mut result = array_str;
                    for idx in indices {
                        let idx_str = self.emit_expr_to_string(idx)?;
                        result = format!("{}[({}) as usize - 1]", result, idx_str);
                    }
                    Ok(result)
                }
            }

            // Range
            AotExpr::Range {
                start, stop, step, ..
            } => {
                let start_str = self.emit_expr_to_string(start)?;
                let stop_str = self.emit_expr_to_string(stop)?;

                if let Some(step_expr) = step {
                    let step_str = self.emit_expr_to_string(step_expr)?;
                    Ok(format!(
                        "({}..={}).step_by({} as usize)",
                        start_str, stop_str, step_str
                    ))
                } else {
                    Ok(format!("({}..={})", start_str, stop_str))
                }
            }

            // Struct construction
            AotExpr::StructNew { name, fields } => {
                let fields_str: Vec<_> = fields
                    .iter()
                    .map(|f| self.emit_expr_to_string(f))
                    .collect::<AotResult<_>>()?;
                Ok(format!("{}::new({})", name, fields_str.join(", ")))
            }

            // Field access
            AotExpr::FieldAccess { object, field, .. } => {
                let obj_str = self.emit_expr_to_string(object)?;
                Ok(format!("{}.{}", obj_str, field))
            }

            // Ternary
            AotExpr::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => {
                let cond_str = self.emit_expr_to_string(condition)?;
                let then_str = self.emit_expr_to_string(then_expr)?;
                let else_str = self.emit_expr_to_string(else_expr)?;
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
                let ty_str = self.type_to_rust(target_ty);

                // Handle type conversions appropriately
                match (&value_ty, target_ty) {
                    // Same type - no conversion needed
                    (a, b) if a == b => Ok(value_str),

                    // Numeric conversions using 'as' keyword
                    (StaticType::I64, StaticType::F64)
                    | (StaticType::I32, StaticType::F64)
                    | (StaticType::I64, StaticType::F32)
                    | (StaticType::I32, StaticType::F32)
                    | (StaticType::F64, StaticType::I64)
                    | (StaticType::F32, StaticType::I64)
                    | (StaticType::F64, StaticType::I32)
                    | (StaticType::F32, StaticType::I32)
                    | (StaticType::I64, StaticType::I32)
                    | (StaticType::I32, StaticType::I64)
                    | (StaticType::F64, StaticType::F32)
                    | (StaticType::F32, StaticType::F64) => {
                        Ok(format!("({} as {})", value_str, ty_str))
                    }

                    // Bool to numeric
                    (StaticType::Bool, StaticType::I64)
                    | (StaticType::Bool, StaticType::I32)
                    | (StaticType::Bool, StaticType::F64)
                    | (StaticType::Bool, StaticType::F32) => {
                        Ok(format!("({} as {})", value_str, ty_str))
                    }

                    // Other conversions - just use 'as' and let Rust handle it
                    _ => Ok(format!("({} as {})", value_str, ty_str)),
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
    fn emit_builtin_call(&self, builtin: &AotBuiltinOp, args: &[String]) -> AotResult<String> {
        match builtin {
            // Basic math functions - use Rust's f64 methods
            AotBuiltinOp::Sqrt => Ok(format!("{}.sqrt()", args[0])),
            AotBuiltinOp::Sin => Ok(format!("{}.sin()", args[0])),
            AotBuiltinOp::Cos => Ok(format!("{}.cos()", args[0])),
            AotBuiltinOp::Tan => Ok(format!("{}.tan()", args[0])),
            AotBuiltinOp::Asin => Ok(format!("{}.asin()", args[0])),
            AotBuiltinOp::Acos => Ok(format!("{}.acos()", args[0])),
            AotBuiltinOp::Atan => Ok(format!("{}.atan()", args[0])),
            AotBuiltinOp::Atan2 => Ok(format!("{}.atan2({})", args[0], args[1])),
            AotBuiltinOp::Exp => Ok(format!("{}.exp()", args[0])),
            AotBuiltinOp::Log => Ok(format!("{}.ln()", args[0])),
            AotBuiltinOp::Abs => Ok(format!("{}.abs()", args[0])),
            AotBuiltinOp::Floor => Ok(format!("{}.floor()", args[0])),
            AotBuiltinOp::Ceil => Ok(format!("{}.ceil()", args[0])),
            AotBuiltinOp::Round => Ok(format!("{}.round()", args[0])),
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
            AotBuiltinOp::Div => Ok(format!("{} / {}", args[0], args[1])),
            AotBuiltinOp::Mod => Ok(format!("{}.rem_euclid({})", args[0], args[1])),
            AotBuiltinOp::Rem => Ok(format!("{} % {}", args[0], args[1])),
            // Note: gcd, lcm removed - now Pure Julia (base/intfuncs.jl)

            // Special value checks
            AotBuiltinOp::Isnan => Ok(format!("{}.is_nan()", args[0])),
            AotBuiltinOp::Isinf => Ok(format!("{}.is_infinite()", args[0])),
            AotBuiltinOp::Isfinite => Ok(format!("{}.is_finite()", args[0])),

            // Array operations
            // length(arr): total number of elements
            // For 1D: arr.len(), for 2D: arr.iter().map(|r| r.len()).sum()
            AotBuiltinOp::Length => Ok(format!("{}.len() as i64", args[0])),
            // size(arr): tuple of dimensions
            // For 1D: (len,), for 2D: (rows, cols)
            // Note: This simplified version works for 1D and row-major 2D arrays
            AotBuiltinOp::Size => {
                if args.len() == 1 {
                    // size(arr) without dimension argument
                    // We generate code that works for both 1D and 2D
                    Ok(format!("({}.len() as i64,)", args[0]))
                } else if args.len() >= 2 {
                    // size(arr, dim) - get specific dimension
                    // For dim=1: rows (outer len), for dim=2: cols (inner len)
                    Ok(format!(
                        "if {} == 1 {{ {}.len() as i64 }} else {{ {}[0].len() as i64 }}",
                        args[1], args[0], args[0]
                    ))
                } else {
                    Ok("(0i64,)".to_string())
                }
            }
            // ndims(arr): number of dimensions
            // This is a simplified implementation that checks if first element is a Vec
            AotBuiltinOp::Ndims => Ok("1i64".to_string()), // Simplified - would need type info for accurate result
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
            // first(tuple) -> tuple[0].clone() (tuples stored as Vec in AoT IR)
            AotBuiltinOp::TupleFirst => Ok(format!("{}[0].clone()", args[0])),
            // last(tuple) -> tuple[tuple.len() - 1].clone()
            AotBuiltinOp::TupleLast => {
                Ok(format!("{}[{}.len() - 1].clone()", args[0], args[0]))
            }
            // isempty(arr) -> arr.is_empty()
            AotBuiltinOp::IsEmpty => Ok(format!("{}.is_empty()", args[0])),
            // collect(iter) -> iter.collect::<Vec<_>>()
            AotBuiltinOp::Collect => Ok(format!("{}.collect::<Vec<_>>()", args[0])),
            // zeros(n) -> 1D array, zeros(m, n) -> 2D matrix
            AotBuiltinOp::Zeros => {
                if args.len() == 1 {
                    // 1D: zeros(n)
                    Ok(format!("vec![0.0_f64; {} as usize]", args[0]))
                } else if args.len() >= 2 {
                    // 2D: zeros(rows, cols) -> Vec<Vec<f64>>
                    Ok(format!(
                        "(0..{} as usize).map(|_| vec![0.0_f64; {} as usize]).collect::<Vec<_>>()",
                        args[0], args[1]
                    ))
                } else {
                    Ok("vec![]".to_string())
                }
            }
            // ones(n) -> 1D array, ones(m, n) -> 2D matrix
            AotBuiltinOp::Ones => {
                if args.len() == 1 {
                    // 1D: ones(n)
                    Ok(format!("vec![1.0_f64; {} as usize]", args[0]))
                } else if args.len() >= 2 {
                    // 2D: ones(rows, cols) -> Vec<Vec<f64>>
                    Ok(format!(
                        "(0..{} as usize).map(|_| vec![1.0_f64; {} as usize]).collect::<Vec<_>>()",
                        args[0], args[1]
                    ))
                } else {
                    Ok("vec![]".to_string())
                }
            }
            // Note: Fill removed — now Pure Julia (Issue #2640)
            AotBuiltinOp::Reshape => Ok(format!("{} /* reshape */", args[0])),
            AotBuiltinOp::Sum => Ok(format!("{}.iter().sum::<f64>()", args[0])),

            // Higher-order functions
            // Note: These expect the function as the first argument, array as second
            // For anonymous functions (closures), the closure syntax is passed directly
            AotBuiltinOp::Map => {
                if args.len() >= 2 {
                    // map(f, arr) -> arr.iter().map(f).collect() for closures
                    // or arr.iter().map(|&x| f(x)).collect() for named functions
                    if Self::is_closure_literal(&args[0]) {
                        // Closure: use .copied() to get values, then apply closure directly
                        Ok(format!(
                            "{}.iter().copied().map({}).collect::<Vec<_>>()",
                            args[1], args[0]
                        ))
                    } else {
                        Ok(format!(
                            "{}.iter().map(|&x| {}(x)).collect::<Vec<_>>()",
                            args[1], args[0]
                        ))
                    }
                } else {
                    Ok("/* map: insufficient args */".to_string())
                }
            }
            AotBuiltinOp::Filter => {
                if args.len() >= 2 {
                    // filter(f, arr) -> arr.iter().filter(f).cloned().collect() for closures
                    // or arr.iter().filter(|&&x| f(x)).cloned().collect() for named functions
                    if Self::is_closure_literal(&args[0]) {
                        Ok(format!(
                            "{}.iter().copied().filter({}).collect::<Vec<_>>()",
                            args[1], args[0]
                        ))
                    } else {
                        Ok(format!(
                            "{}.iter().filter(|&&x| {}(x)).cloned().collect::<Vec<_>>()",
                            args[1], args[0]
                        ))
                    }
                } else {
                    Ok("/* filter: insufficient args */".to_string())
                }
            }
            AotBuiltinOp::Reduce => {
                if args.len() >= 2 {
                    // reduce(f, arr) -> arr.iter().cloned().reduce(f)
                    // For operators like +, we need special handling
                    if args[0] == "+" {
                        Ok(format!(
                            "{}.iter().cloned().reduce(|a, b| a + b).unwrap_or_default()",
                            args[1]
                        ))
                    } else if args[0] == "*" {
                        Ok(format!(
                            "{}.iter().cloned().reduce(|a, b| a * b).unwrap_or(1)",
                            args[1]
                        ))
                    } else if Self::is_closure_literal(&args[0]) {
                        // Closure: pass directly to reduce
                        Ok(format!(
                            "{}.iter().cloned().reduce({}).unwrap_or_default()",
                            args[1], args[0]
                        ))
                    } else {
                        Ok(format!(
                            "{}.iter().cloned().reduce(|a, b| {}(a, b)).unwrap_or_default()",
                            args[1], args[0]
                        ))
                    }
                } else {
                    Ok("/* reduce: insufficient args */".to_string())
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

            // I/O operations
            // Generate format string with one {} for each argument
            AotBuiltinOp::Println => {
                let format_specifiers: String =
                    args.iter().map(|_| "{}").collect::<Vec<_>>().join("");
                Ok(format!(
                    "println!(\"{}\", {})",
                    format_specifiers,
                    args.join(", ")
                ))
            }
            AotBuiltinOp::Print => {
                let format_specifiers: String =
                    args.iter().map(|_| "{}").collect::<Vec<_>>().join("");
                Ok(format!(
                    "print!(\"{}\", {})",
                    format_specifiers,
                    args.join(", ")
                ))
            }

            // Type operations
            AotBuiltinOp::TypeOf => Ok(format!("std::any::type_name_of_val(&{})", args[0])),
            AotBuiltinOp::Isa => Ok(format!("/* isa check */ true")),

            // Random (simplified)
            AotBuiltinOp::Rand => Ok("rand::random::<f64>()".to_string()),
            AotBuiltinOp::Randn => Ok("/* randn */ 0.0".to_string()),

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
            // fptosi(Int64, x) -> x as i64
            AotBuiltinOp::Fptosi => {
                // Second argument is the value to convert (first is the type)
                if args.len() >= 2 {
                    Ok(format!("({} as i64)", args[1]))
                } else if args.len() == 1 {
                    Ok(format!("({} as i64)", args[0]))
                } else {
                    Ok("/* fptosi: missing args */ 0_i64".to_string())
                }
            }
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
