use crate::ir::core::{Expr, Literal, Stmt};
use crate::span::Span;
use crate::vm::{StructInstance, Value};

/// Convert a Value to a Literal for IR injection.
pub(crate) fn value_to_literal(value: &Value) -> Option<Literal> {
    // Handle Complex structs first - convert to Literal::Struct
    if value.is_complex() {
        if let Some((re, im)) = value.as_complex_parts() {
            return Some(Literal::Struct(
                "Complex{Float64}".to_string(),
                vec![Literal::Float(re), Literal::Float(im)],
            ));
        }
    }
    match value {
        Value::I64(v) => Some(Literal::Int(*v)),
        Value::F64(v) => Some(Literal::Float(*v)),
        Value::Str(v) => Some(Literal::Str(v.clone())),
        Value::Array(arr) => {
            let arr = arr.borrow();
            // Convert ArrayData to appropriate Literal variant preserving element type
            match &arr.data {
                crate::vm::ArrayData::F64(v) => Some(Literal::Array(v.clone(), arr.shape.clone())),
                crate::vm::ArrayData::I64(v) => {
                    Some(Literal::ArrayI64(v.clone(), arr.shape.clone()))
                }
                crate::vm::ArrayData::Bool(v) => {
                    Some(Literal::ArrayBool(v.clone(), arr.shape.clone()))
                }
                // For complex arrays (StructRefs), we can't convert to Literal::Array
                // but the array is still stored in REPLGlobals, so return None here
                // The array will be available via get() but won't be injected as a literal
                _ => None, // Complex, String, Char, Any arrays not supported yet
            }
        }
        // Memory → Array (Issue #2764)
        Value::Memory(mem) => {
            let arr = crate::vm::util::memory_to_array_ref(mem);
            value_to_literal(&Value::Array(arr))
        }
        Value::Nothing => Some(Literal::Nothing),
        Value::Missing => Some(Literal::Missing),
        Value::Bool(v) => Some(Literal::Bool(*v)),
        Value::Char(v) => Some(Literal::Char(*v)),
        // Narrow integer types — inject as Literal::Int (i64) or Literal::Int128 (Issue #3296)
        // NOTE: I8/I16/I32/U8/U16/U32/U64 widen to I64 on re-injection (type narrowing lost).
        // This is intentional: value preservation is more important than exact type retention.
        // U64 and U128 values larger than i64::MAX / i128::MAX will truncate — acceptable
        // since the REPL is interactive and such values are rare edge cases.
        Value::I8(v) => Some(Literal::Int(*v as i64)),
        Value::I16(v) => Some(Literal::Int(*v as i64)),
        Value::I32(v) => Some(Literal::Int(*v as i64)),
        Value::I128(v) => Some(Literal::Int128(*v)),
        Value::U8(v) => Some(Literal::Int(*v as i64)),
        Value::U16(v) => Some(Literal::Int(*v as i64)),
        Value::U32(v) => Some(Literal::Int(*v as i64)),
        Value::U64(v) => Some(Literal::Int(*v as i64)),
        Value::U128(v) => Some(Literal::Int128(*v as i128)),
        Value::F32(v) => Some(Literal::Float32(*v)),
        // Float16 — preserved with full type fidelity via Literal::Float16 (Issue #3309)
        Value::F16(v) => Some(Literal::Float16(*v)),
        // Regex — Literal::Regex { pattern, flags } exists and compiles to PushRegex (Issue #3299)
        Value::Regex(rv) => Some(Literal::Regex {
            pattern: rv.pattern.clone(),
            flags: rv.flags.clone(),
        }),
        // Metaprogramming types for REPL persistence
        Value::Symbol(sym) => Some(Literal::Symbol(sym.as_str().to_string())),
        Value::Expr(expr) => {
            // Recursively convert ExprValue to Literal::Expr
            let head = expr.head.as_str().to_string();
            let args = expr
                .args
                .iter()
                .map(value_to_literal)
                .collect::<Option<Vec<_>>>()?;
            Some(Literal::Expr { head, args })
        }
        Value::QuoteNode(inner) => {
            let inner_lit = value_to_literal(inner)?;
            Some(Literal::QuoteNode(Box::new(inner_lit)))
        }
        Value::LineNumberNode(lnn) => Some(Literal::LineNumberNode {
            line: lnn.line,
            file: lnn.file.clone(),
        }),
        // Enum — Literal::Enum { type_name, value } compiles to PushEnum (Issue #3302)
        Value::Enum { type_name, value } => Some(Literal::Enum {
            type_name: type_name.clone(),
            value: *value,
        }),
        // Structs are handled specially via struct_instance_to_literal
        // Range, Tuple, Dict, etc. would need special handling
        _ => None,
    }
}

/// Convert a callable Value (Function, ComposedFunction, or Closure) to an Expr for IR injection.
/// Returns None for non-callable values.
pub(crate) fn callable_value_to_expr(value: &Value, span: Span) -> Option<Expr> {
    match value {
        Value::Function(fv) => Some(Expr::FunctionRef {
            name: fv.name.clone(),
            span,
        }),
        Value::ComposedFunction(cf) => {
            // Recursively convert outer and inner to expressions
            let outer_expr = callable_value_to_expr(&cf.outer, span)?;
            let inner_expr = callable_value_to_expr(&cf.inner, span)?;
            Some(Expr::Call {
                function: "compose".to_string(),
                args: vec![outer_expr, inner_expr],
                kwargs: Vec::new(),
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span,
            })
        }
        // Closures are injected as FunctionRefs; the underlying function is preserved in
        // REPLSession::functions and merged into the next program (Issue #3283).
        // Captured variables are already stored as separate REPL globals and will be
        // re-injected, causing the VM to re-create the closure automatically.
        Value::Closure(cv) => Some(Expr::FunctionRef {
            name: cv.name.clone(),
            span,
        }),
        _ => None,
    }
}

/// Convert a struct instance to a Literal::Struct.
/// Returns None if any field value cannot be converted to a literal.
///
/// NOTE: Field type coverage is automatically kept in sync with `value_to_literal()`
/// because this function delegates field conversion to it. When a new injectable type
/// is added to `value_to_literal()`, struct fields of that type automatically persist
/// without any changes here. (Issue #3314)
pub(crate) fn struct_instance_to_literal(
    instance: &StructInstance,
    struct_name: &str,
) -> Option<Literal> {
    let mut field_literals = Vec::with_capacity(instance.values.len());
    for value in &instance.values {
        // Delegate field conversion to value_to_literal() for consistent coverage.
        // value_to_literal() handles Complex (via is_complex() check), Array (with element
        // type preservation), Memory, and all primitive types. Any type that value_to_literal()
        // cannot convert causes the whole struct to fail persistence.
        match value_to_literal(value) {
            Some(lit) => field_literals.push(lit),
            None => return None,
        }
    }
    Some(Literal::Struct(struct_name.to_string(), field_literals))
}

/// Extract variable names that are assigned in a list of statements.
pub(crate) fn extract_assigned_variables(stmts: &[Stmt]) -> Vec<String> {
    let mut vars = Vec::new();
    for stmt in stmts {
        match stmt {
            Stmt::Assign { var, value, .. } => {
                vars.push(var.clone());
                // Also check the value expression for nested assignments (e.g., local result = x = 42)
                vars.extend(extract_assigned_from_expr(value));
            }
            Stmt::Block(block) => {
                vars.extend(extract_assigned_variables(&block.stmts));
            }
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                vars.extend(extract_assigned_variables(&then_branch.stmts));
                if let Some(else_b) = else_branch {
                    vars.extend(extract_assigned_variables(&else_b.stmts));
                }
            }
            Stmt::While { body, .. } => {
                vars.extend(extract_assigned_variables(&body.stmts));
            }
            Stmt::For { body, .. } => {
                vars.extend(extract_assigned_variables(&body.stmts));
            }
            Stmt::Try {
                try_block,
                catch_block,
                finally_block,
                ..
            } => {
                vars.extend(extract_assigned_variables(&try_block.stmts));
                if let Some(catch_b) = catch_block {
                    vars.extend(extract_assigned_variables(&catch_b.stmts));
                }
                if let Some(finally_b) = finally_block {
                    vars.extend(extract_assigned_variables(&finally_b.stmts));
                }
            }
            Stmt::Timed { body, .. } => {
                vars.extend(extract_assigned_variables(&body.stmts));
            }
            // Handle Expr statements that may contain AssignExpr
            Stmt::Expr { expr, .. } => {
                vars.extend(extract_assigned_from_expr(expr));
            }
            _ => {}
        }
    }
    vars
}

/// Extract variable names from AssignExpr inside an expression.
/// This handles expressions like `x = 42` or `local result = x = 42` where x = 42 is an AssignExpr.
fn extract_assigned_from_expr(expr: &Expr) -> Vec<String> {
    let mut vars = Vec::new();

    match expr {
        Expr::AssignExpr { var, value, .. } => {
            // This is an assignment expression - the variable is being assigned
            vars.push(var.clone());
            // Also check the value for nested assignments
            vars.extend(extract_assigned_from_expr(value));
        }
        Expr::BinaryOp { left, right, .. } => {
            vars.extend(extract_assigned_from_expr(left));
            vars.extend(extract_assigned_from_expr(right));
        }
        Expr::UnaryOp { operand, .. } => {
            vars.extend(extract_assigned_from_expr(operand));
        }
        Expr::Call { args, kwargs, .. } => {
            for arg in args {
                vars.extend(extract_assigned_from_expr(arg));
            }
            for (_, kwarg_val) in kwargs {
                vars.extend(extract_assigned_from_expr(kwarg_val));
            }
        }
        Expr::Builtin { args, .. } => {
            for arg in args {
                vars.extend(extract_assigned_from_expr(arg));
            }
        }
        Expr::TupleLiteral { elements, .. } => {
            for e in elements {
                vars.extend(extract_assigned_from_expr(e));
            }
        }
        Expr::LetBlock { body, .. } => {
            // Extract assigned variables from the body statements of a LetBlock
            // This is important for macro expansions that produce LetBlock expressions
            vars.extend(extract_assigned_variables(&body.stmts));
        }
        _ => {}
    }
    vars
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::core::Literal;
    use crate::vm::Value;

    // Issue #3296: narrow int types must produce Some from value_to_literal

    #[test]
    fn test_value_to_literal_i8() {
        assert!(
            matches!(value_to_literal(&Value::I8(10)), Some(Literal::Int(10))),
            "Expected Literal::Int(10) for I8(10)"
        );
    }

    #[test]
    fn test_value_to_literal_i16() {
        assert!(
            matches!(value_to_literal(&Value::I16(1000)), Some(Literal::Int(1000))),
            "Expected Literal::Int(1000) for I16(1000)"
        );
    }

    #[test]
    fn test_value_to_literal_i32() {
        assert!(
            matches!(value_to_literal(&Value::I32(42)), Some(Literal::Int(42))),
            "Expected Literal::Int(42) for I32(42)"
        );
    }

    #[test]
    fn test_value_to_literal_i128() {
        assert!(
            matches!(value_to_literal(&Value::I128(i128::MAX)), Some(Literal::Int128(v)) if v == i128::MAX),
            "Expected Literal::Int128(i128::MAX) for I128(i128::MAX)"
        );
    }

    #[test]
    fn test_value_to_literal_u8() {
        assert!(
            matches!(value_to_literal(&Value::U8(200)), Some(Literal::Int(200))),
            "Expected Literal::Int(200) for U8(200)"
        );
    }

    #[test]
    fn test_value_to_literal_u32() {
        assert!(
            matches!(value_to_literal(&Value::U32(99)), Some(Literal::Int(99))),
            "Expected Literal::Int(99) for U32(99)"
        );
    }

    #[test]
    fn test_value_to_literal_u128() {
        assert!(
            matches!(value_to_literal(&Value::U128(0)), Some(Literal::Int128(0))),
            "Expected Literal::Int128(0) for U128(0)"
        );
    }

    #[test]
    fn test_value_to_literal_f32() {
        let result = value_to_literal(&Value::F32(1.25_f32));
        assert!(
            matches!(result, Some(Literal::Float32(v)) if (v - 1.25_f32).abs() < 1e-6),
            "Expected Literal::Float32(1.25) for F32(1.25), got {:?}",
            result
        );
    }

    // Issue #3309: Float16 must produce Literal::Float16 (not widened to Float32)

    #[test]
    fn test_value_to_literal_f16_produces_float16() {
        let f16_val = half::f16::from_f32(1.5_f32);
        let result = value_to_literal(&Value::F16(f16_val));
        assert!(
            matches!(result, Some(Literal::Float16(v)) if (v.to_f32() - 1.5_f32).abs() < 1e-4),
            "Expected Literal::Float16(~1.5) for F16(1.5), got {:?}",
            result
        );
    }

    #[test]
    fn test_value_to_literal_f16_zero() {
        let f16_zero = half::f16::from_f32(0.0_f32);
        let result = value_to_literal(&Value::F16(f16_zero));
        assert!(
            matches!(result, Some(Literal::Float16(v)) if v.to_f32() == 0.0_f32),
            "Expected Literal::Float16(0.0) for F16(0.0), got {:?}",
            result
        );
    }

    #[test]
    fn test_value_to_literal_negative_i32() {
        assert!(
            matches!(value_to_literal(&Value::I32(-1)), Some(Literal::Int(-1))),
            "Expected Literal::Int(-1) for I32(-1)"
        );
    }

    // Issue #3299: Regex persistence
    #[test]
    fn test_value_to_literal_regex_simple() {
        use crate::vm::value::RegexValue;
        let rv = RegexValue::new("hello", "").unwrap();
        let result = value_to_literal(&Value::Regex(rv));
        assert!(
            matches!(result, Some(Literal::Regex { ref pattern, ref flags }) if pattern == "hello" && flags.is_empty()),
            "Expected Literal::Regex(hello, ''), got {:?}",
            result
        );
    }

    #[test]
    fn test_value_to_literal_regex_with_flags() {
        use crate::vm::value::RegexValue;
        let rv = RegexValue::new("world", "i").unwrap();
        let result = value_to_literal(&Value::Regex(rv));
        assert!(
            matches!(result, Some(Literal::Regex { ref pattern, ref flags }) if pattern == "world" && flags == "i"),
            "Expected Literal::Regex(world, 'i'), got {:?}",
            result
        );
    }

    // Issue #3302: @enum values must produce Some from value_to_literal

    #[test]
    fn test_value_to_literal_enum_basic() {
        let result = value_to_literal(&Value::Enum {
            type_name: "Color".to_string(),
            value: 1,
        });
        assert!(
            matches!(result, Some(Literal::Enum { ref type_name, value: 1 }) if type_name == "Color"),
            "Expected Literal::Enum(Color, 1), got {:?}",
            result
        );
    }

    #[test]
    fn test_value_to_literal_enum_zero_value() {
        let result = value_to_literal(&Value::Enum {
            type_name: "Status".to_string(),
            value: 0,
        });
        assert!(
            matches!(result, Some(Literal::Enum { ref type_name, value: 0 }) if type_name == "Status"),
            "Expected Literal::Enum(Status, 0), got {:?}",
            result
        );
    }

    #[test]
    fn test_value_to_literal_enum_negative_value() {
        let result = value_to_literal(&Value::Enum {
            type_name: "Direction".to_string(),
            value: -1,
        });
        assert!(
            matches!(result, Some(Literal::Enum { ref type_name, value: -1 }) if type_name == "Direction"),
            "Expected Literal::Enum(Direction, -1), got {:?}",
            result
        );
    }

    // Issue #3298: Completeness test — every Value variant stored in other_vars
    // that has a Literal counterpart MUST return Some from value_to_literal().
    // When a new injectable type is added to other_vars, add it here too.
    // When a new Value variant is added to other_vars WITHOUT a Literal counterpart,
    // add it to the non_injectable list below with a // TODO comment.
    #[test]
    fn test_all_other_vars_injectable_types_return_some() {
        use crate::vm::value::RegexValue;

        // Each entry: (human-readable name, Value to test)
        // These types are stored in other_vars AND have a Literal representation.
        let injectable: &[(&str, Value)] = &[
            ("Bool", Value::Bool(true)),
            ("I8", Value::I8(1)),
            ("I16", Value::I16(1)),
            ("I32", Value::I32(1)),
            ("I128", Value::I128(1)),
            ("U8", Value::U8(1)),
            ("U16", Value::U16(1)),
            ("U32", Value::U32(1)),
            ("U64", Value::U64(1)),
            ("U128", Value::U128(1)),
            // F16 preserved as Literal::Float16 (Issue #3309)
            ("F16", Value::F16(half::f16::from_f32(1.5))),
            ("F32", Value::F32(1.0)),
            ("Char", Value::Char('a')),
            (
                "Regex",
                Value::Regex(RegexValue::new("test", "").expect("valid regex")),
            ),
            (
                "Enum",
                Value::Enum {
                    type_name: "Color".to_string(),
                    value: 1,
                },
            ),
        ];

        for (name, val) in injectable {
            let result = value_to_literal(val);
            assert!(
                result.is_some(),
                "value_to_literal returned None for {} (Issue #3298, #3305)",
                name
            );
        }

        // Non-injectable types stored in other_vars (no Literal representation yet).
        // These are documented here so that the absence is intentional and tracked.
        // When a type moves from non-injectable to injectable, remove it from this list
        // and add it to the injectable list above.
        //
        // - Value::GlobalRef: no Literal::GlobalRef exists (Issue #3301)
        // - Value::Pairs: no Literal::Pairs exists (Issue #3301)
        // - Value::Set: no Literal::Set exists (Issue #3301)
        // - Value::RegexMatch: no Literal::RegexMatch exists (Issue #3301)
        // - Value::Memory: no Literal::Memory exists (Issue #3301)
        // - Value::Closure: injected via callable_value_to_expr(), not value_to_literal()
    }

    // Issue #3310: struct_instance_to_literal must handle Bool/narrow-int/Char/Enum fields

    #[test]
    fn test_struct_instance_bool_field() {
        use crate::vm::StructInstance;
        let instance = StructInstance::new(0, vec![Value::Bool(true)]);
        let result = struct_instance_to_literal(&instance, "MyStruct");
        assert!(
            matches!(result, Some(Literal::Struct(ref name, ref fields))
                if name == "MyStruct" && matches!(fields[0], Literal::Bool(true))),
            "Expected Literal::Struct with Bool field, got {:?}",
            result
        );
    }

    #[test]
    fn test_struct_instance_i32_field() {
        use crate::vm::StructInstance;
        let instance = StructInstance::new(0, vec![Value::I32(42)]);
        let result = struct_instance_to_literal(&instance, "MyStruct");
        assert!(
            matches!(result, Some(Literal::Struct(_, ref fields)) if matches!(fields[0], Literal::Int(42))),
            "Expected Literal::Int(42) for I32 field, got {:?}",
            result
        );
    }

    #[test]
    fn test_struct_instance_char_field() {
        use crate::vm::StructInstance;
        let instance = StructInstance::new(0, vec![Value::Char('z')]);
        let result = struct_instance_to_literal(&instance, "MyStruct");
        assert!(
            matches!(result, Some(Literal::Struct(_, ref fields)) if matches!(fields[0], Literal::Char('z'))),
            "Expected Literal::Char('z') for Char field, got {:?}",
            result
        );
    }

    #[test]
    fn test_struct_instance_enum_field() {
        use crate::vm::StructInstance;
        let instance = StructInstance::new(0, vec![Value::Enum {
            type_name: "Color".to_string(),
            value: 2,
        }]);
        let result = struct_instance_to_literal(&instance, "Pixel");
        assert!(
            matches!(
                result,
                Some(Literal::Struct(_, ref fields))
                    if matches!(&fields[0], Literal::Enum { ref type_name, value: 2 } if type_name == "Color")
            ),
            "Expected Literal::Enum field in struct, got {:?}",
            result
        );
    }

    #[test]
    fn test_struct_instance_f32_field() {
        use crate::vm::StructInstance;
        let instance = StructInstance::new(0, vec![Value::F32(1.5_f32)]);
        let result = struct_instance_to_literal(&instance, "MyStruct");
        assert!(
            matches!(result, Some(Literal::Struct(_, ref fields))
                if matches!(fields[0], Literal::Float32(v) if (v - 1.5_f32).abs() < 1e-6)),
            "Expected Literal::Float32(1.5) for F32 field, got {:?}",
            result
        );
    }

    #[test]
    fn test_struct_instance_mixed_fields() {
        use crate::vm::StructInstance;
        // Struct with I64, Bool, Char, Enum fields — all must be preserved (Issue #3310)
        let instance = StructInstance::new(0, vec![
            Value::I64(10),
            Value::Bool(false),
            Value::Char('a'),
            Value::Enum { type_name: "Status".to_string(), value: 0 },
        ]);
        let result = struct_instance_to_literal(&instance, "Complex");
        assert!(
            result.is_some(),
            "Expected Some for struct with I64/Bool/Char/Enum fields, got None (Issue #3310)"
        );
        if let Some(Literal::Struct(name, fields)) = result {
            assert_eq!(name, "Complex");
            assert_eq!(fields.len(), 4);
        }
    }

    // Issue #3316: Verify that struct field delegation to value_to_literal() covers all
    // injectable types. When a new type is added to value_to_literal(), this test ensures
    // it automatically works as a struct field (due to the delegation design from #3314).
    #[test]
    fn test_struct_instance_auto_sync_with_value_to_literal() {
        use crate::vm::StructInstance;
        // Every type injectable via value_to_literal() must also work as struct field
        // due to the delegation design from Issue #3314. This test verifies the contract.
        let injectable_values: &[(&str, Value)] = &[
            ("Bool", Value::Bool(true)),
            ("I8", Value::I8(1)),
            ("I16", Value::I16(1)),
            ("I32", Value::I32(1)),
            ("I64", Value::I64(1)),
            ("I128", Value::I128(1)),
            ("U8", Value::U8(1)),
            ("U16", Value::U16(1)),
            ("U32", Value::U32(1)),
            ("U64", Value::U64(1)),
            ("U128", Value::U128(1)),
            // F16 preserved as Literal::Float16 (Issue #3309)
            ("F16", Value::F16(half::f16::from_f32(1.0))),
            ("F32", Value::F32(1.0)),
            ("F64", Value::F64(1.0)),
            ("Char", Value::Char('x')),
            ("Str", Value::Str("hi".to_string())),
            ("Nothing", Value::Nothing),
            ("Missing", Value::Missing),
            (
                "Enum",
                Value::Enum {
                    type_name: "T".to_string(),
                    value: 0,
                },
            ),
        ];

        for (name, val) in injectable_values {
            let instance = StructInstance::new(0, vec![val.clone()]);
            let result = struct_instance_to_literal(&instance, "Test");
            assert!(
                result.is_some(),
                "Field {:?} ({}) should be injectable in struct via delegation (Issue #3316)",
                val,
                name
            );
        }
    }

    // Issue #3320: Verify that value_to_literal returns type-faithful Literal variants.
    // Each Value type must map to its *exact* Literal counterpart — NOT a widened type.
    // E.g., F16 → Float16, F32 → Float32 (NOT Float64), I32 → Int (widening acceptable).
    #[test]
    fn test_value_to_literal_type_fidelity() {
        // F16 → Float16 (NOT Float32)
        assert!(
            matches!(
                value_to_literal(&Value::F16(half::f16::from_f32(1.0))),
                Some(Literal::Float16(_))
            ),
            "F16 must produce Literal::Float16, not a widened type (Issue #3320)"
        );
        // F32 → Float32 (NOT Float64)
        assert!(
            matches!(
                value_to_literal(&Value::F32(1.25_f32)),
                Some(Literal::Float32(_))
            ),
            "F32 must produce Literal::Float32, not Float64 (Issue #3320)"
        );
        // F64 → Float
        assert!(
            matches!(
                value_to_literal(&Value::F64(1.5)),
                Some(Literal::Float(_))
            ),
            "F64 must produce Literal::Float (Issue #3320)"
        );
        // I64 → Int
        assert!(
            matches!(value_to_literal(&Value::I64(42)), Some(Literal::Int(42))),
            "I64 must produce Literal::Int (Issue #3320)"
        );
        // I128 → Int128
        assert!(
            matches!(
                value_to_literal(&Value::I128(100)),
                Some(Literal::Int128(100))
            ),
            "I128 must produce Literal::Int128 (Issue #3320)"
        );
        // Bool → Bool
        assert!(
            matches!(
                value_to_literal(&Value::Bool(true)),
                Some(Literal::Bool(true))
            ),
            "Bool must produce Literal::Bool (Issue #3320)"
        );
        // Char → Char
        assert!(
            matches!(
                value_to_literal(&Value::Char('x')),
                Some(Literal::Char('x'))
            ),
            "Char must produce Literal::Char (Issue #3320)"
        );
        // Enum → Enum
        assert!(
            matches!(
                value_to_literal(&Value::Enum {
                    type_name: "Color".to_string(),
                    value: 1,
                }),
                Some(Literal::Enum { value: 1, .. })
            ),
            "Enum must produce Literal::Enum (Issue #3320)"
        );
        // Nothing → Nothing
        assert!(
            matches!(
                value_to_literal(&Value::Nothing),
                Some(Literal::Nothing)
            ),
            "Nothing must produce Literal::Nothing (Issue #3320)"
        );
        // Missing → Missing
        assert!(
            matches!(
                value_to_literal(&Value::Missing),
                Some(Literal::Missing)
            ),
            "Missing must produce Literal::Missing (Issue #3320)"
        );
        // Str → Str
        assert!(
            matches!(
                value_to_literal(&Value::Str("hi".to_string())),
                Some(Literal::Str(_))
            ),
            "Str must produce Literal::Str (Issue #3320)"
        );
    }
}
