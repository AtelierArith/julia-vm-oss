use std::borrow::Cow;

use crate::ir::core::{Expr, Literal, Stmt};
use crate::span::Span;
use crate::vm::value::{array_wrapper_shape_and_offset, MemoryValue};
use crate::vm::{ArrayElementType, ArrayValue, StructInstance, Value};

/// Convert a Value to a Literal for IR injection.
pub(crate) fn value_to_literal(value: &Value) -> Option<Literal> {
    if let Some(arr) = crate::vm::value::native_array_value_ref(value) {
        let arr = arr.borrow();
        return array_value_to_literal(&arr);
    }
    match value {
        Value::I64(v) => Some(Literal::Int(*v)),
        Value::F64(v) => Some(Literal::Float(*v)),
        Value::Str(v) => Some(Literal::Str(v.clone())),
        Value::Memory(mem) => {
            let mem = mem.borrow();
            memory_value_to_literal(&mem)
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
                .args_snapshot()
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
        // Bare struct values (including Complex{Float64}/Complex{Int}/Complex{Float32},
        // Rational, and user structs) convert generically via struct_instance_to_literal,
        // preserving the real struct_name and per-field literal types. This replaces the
        // former Complex-specific special-case that hardcoded "Complex{Float64}" with
        // lossy Literal::Float fields (Issue #5163). Complex globals normally come back
        // from the VM as Value::StructRef and are persisted via the StructRef path in
        // extract_globals_from_vm; this arm covers the remaining cases where a whole
        // struct value is passed (NamedTuple fields, Expr args, ans).
        Value::Struct(s) => struct_instance_to_literal(s, &s.struct_name),
        // Range, Tuple, Dict, etc. would need special handling
        _ => None,
    }
}

/// Recursively reconstruct a runtime `Value` as an injectable `Expr`, including
/// nested arrays (`Any[]` holding a `Vector{Series}`) and struct elements that
/// the flat `value_to_literal` path cannot express. Used to persist module-level
/// mutable state (e.g. `Plots._CURRENT_SERIES`) across REPL evaluations so that
/// `plot!`/`scatter!` keep appending to the current plot (Issue #5296).
///
/// Returns `None` — meaning "do not persist; keep the module's own initializer" —
/// when any element cannot be reconstructed, or when an array is empty (the empty
/// form is exactly what the module initializer already produces).
pub(crate) fn value_to_init_expr(
    value: &Value,
    heap: &[StructInstance],
    span: Span,
) -> Option<Expr> {
    // Top-level entry: an empty array defers to a module initializer (Issue #5296).
    value_to_init_expr_inner(value, heap, span, false)
}

/// `nested` is `true` when reconstructing a value that lives *inside* another
/// value (a struct field or an array element). At top level an empty array
/// returns `None` so a module's own initializer wins (Issue #5296); nested, an
/// empty array MUST be rebuilt as an empty typed array, otherwise the whole
/// enclosing struct fails to persist — which dropped a REPL global holding an
/// array of `Plot`s after the struct grew an empty `hlines`/`vlines = Float64[]`
/// field (Issue #8086 / #8063).
fn value_to_init_expr_inner(
    value: &Value,
    heap: &[StructInstance],
    span: Span,
    nested: bool,
) -> Option<Expr> {
    // Primitives, numeric/bool arrays, Memory, symbols, nothing, and bare structs
    // whose fields are all literal-able round-trip through value_to_literal.
    if let Some(lit) = value_to_literal(value) {
        return Some(Expr::Literal(lit, span));
    }

    // A function/closure value held inside a persisted value (e.g. an ODEProblem's
    // `f` field, `g = sin`) reconstructs as a `FunctionRef` by name; the function
    // itself is preserved in `REPLSession::functions` and re-injected. Without this
    // a struct carrying a function field had no init expr and was dropped.
    if let Some(expr) = callable_value_to_expr(value, span) {
        return Some(expr);
    }

    // Heterogeneous (`Any`) / struct-element arrays and `Array` wrappers: rebuild
    // as a nested ArrayLiteral, recursing on each element. Checked before the
    // struct arm so `Vector{Series}`/`Array{...}` wrappers (which are structs)
    // take the array path rather than being treated as constructible structs.
    if let Some(arr) = value_as_array(value, heap) {
        let elements = arr.to_value_vec();
        if elements.is_empty() {
            // Top-level empty arrays defer to a module initializer (Issue #5296);
            // a nested empty array (e.g. a `Plot.hlines = Float64[]` struct field)
            // must be reconstructed or the enclosing struct fails to persist and
            // the whole global is dropped (Issue #8086).
            return if nested {
                empty_array_init_expr(value, heap, span)
            } else {
                None
            };
        }
        let exprs = elements
            .iter()
            .map(|e| value_to_init_expr_inner(e, heap, span, true))
            .collect::<Option<Vec<_>>>()?;
        return Some(Expr::ArrayLiteral {
            elements: exprs,
            shape: arr.shape.clone(),
            span,
        });
    }

    // A bare tuple (`tspan = (0.0, 60.0)`) has no `Literal` form; rebuild it as a
    // `TupleLiteral`, recursing on each element. Without this a tuple-valued REPL
    // global was dropped and the next eval raised `UndefVarError` (Issue #8243).
    // Mirrors the NamedTuple / Range cases.
    if let Value::Tuple(t) = value {
        let elements = t
            .elements
            .iter()
            .map(|e| value_to_init_expr_inner(e, heap, span, true))
            .collect::<Option<Vec<_>>>()?;
        return Some(Expr::TupleLiteral { elements, span });
    }

    // StaticArrays `@SVector` / `@SMatrix` are stored as an inline static-array
    // value (not a tuple/struct), so they had no reconstruction and were dropped —
    // `u = @SVector [...]` then `u` raised `UndefVarError` (Issue #8249). Rebuild
    // from the column-major elements via the same constructor calls the lowering
    // emits: `SVector(xs...)` for a vector, `SMatrix{M,N}(xs...)` for a matrix
    // (the IR encodes the type parameters in the function-name string).
    if let Value::StaticArrayInline(sa) = value {
        let args = sa
            .to_tuple_value()
            .elements
            .iter()
            .map(|e| value_to_init_expr_inner(e, heap, span, true))
            .collect::<Option<Vec<_>>>()?;
        let arity = args.len();
        let function = if sa.is_vector() {
            "SVector".to_string()
        } else {
            format!("SMatrix{{{},{}}}", sa.rows, sa.cols)
        };
        return Some(Expr::Call {
            function,
            args,
            kwargs: Vec::new(),
            splat_mask: vec![false; arity],
            kwargs_splat_mask: Vec::new(),
            span,
        });
    }

    if let Value::NamedTuple(nt) = value {
        let fields = nt
            .names
            .iter()
            .zip(nt.values.iter())
            .map(|(name, field_value)| {
                value_to_init_expr_inner(field_value, heap, span, true)
                    .map(|expr| (name.clone(), expr))
            })
            .collect::<Option<Vec<_>>>()?;
        return Some(Expr::NamedTupleLiteral { fields, span });
    }

    // A non-array struct (e.g. a `Series`). Fields that are not literal-able —
    // such as broadcast/range-backed `Array` wrappers in a 3D series' x/y/z —
    // are reconstructed recursively and the struct is rebuilt via a positional
    // constructor call `StructName(field0, field1, ...)`.
    // A `Range` field (e.g. `plot!(x, y, t)` stores the range `t` as the series'
    // z without collecting it). Rebuild the `start:step:stop` expression, mirroring
    // the user-global Range path in `inject_globals`.
    if let Value::Range(r) = value {
        let lit = |x: f64| {
            if r.is_float {
                Literal::Float(x)
            } else {
                Literal::Int(x as i64)
            }
        };
        return Some(Expr::Range {
            start: Box::new(Expr::Literal(lit(r.start), span)),
            step: Some(Box::new(Expr::Literal(lit(r.step), span))),
            stop: Box::new(Expr::Literal(lit(r.stop), span)),
            span,
        });
    }

    if let Some(instance) = as_struct(value, heap) {
        let short_name = short_constructible_type_name(&instance.struct_name);
        let args = instance
            .values
            .iter()
            .map(|v| value_to_init_expr_inner(v, heap, span, true))
            .collect::<Option<Vec<_>>>()?;
        let arity = args.len();
        return Some(Expr::Call {
            function: short_name.into_owned(),
            args,
            kwargs: Vec::new(),
            splat_mask: vec![false; arity],
            kwargs_splat_mask: Vec::new(),
            span,
        });
    }

    None
}

/// Re-create a REPL global bound to an **empty** array (`x = []`, `Int[]`,
/// `Any[]`, `Float64[]`) as an `Expr::TypedEmptyArray` init expression.
///
/// `value_to_init_expr` deliberately returns `None` for empty arrays so a
/// module's own initializer wins for module-level state (`_CURRENT_SERIES`,
/// Issue #5296). But a *user* global like `ps = []` has no initializer to fall
/// back on, so without this it is silently dropped and the next evaluation sees
/// `UndefVarError` — which is exactly what breaks `@gif`/`push!(ps, …)` in the
/// REPL (Issue #7151). The element type is parsed from the array's `struct_name`
/// (`Array{Any, 1}` → `Any`, `Array{Int64, 1}` → `Int64`) so `eltype`/`push!`
/// keep matching upstream. Only 1-D (vector) empties are handled; higher-rank
/// empties return `None` (no worse than today) since `TypedEmptyArray` builds a
/// `Vector`.
pub(crate) fn empty_array_init_expr(
    value: &Value,
    heap: &[StructInstance],
    span: Span,
) -> Option<Expr> {
    let arr = value_as_array(value, heap)?;
    if arr.element_count() != 0 || arr.shape.len() > 1 {
        return None;
    }
    let element_type = array_value_element_type_name(value);
    Some(Expr::TypedEmptyArray { element_type, span })
}

/// Best-effort Julia element-type name for an array `value`, parsed from its
/// `struct_name` (`Array{T, N}` / `Vector{T}`). Falls back to `Any`.
fn array_value_element_type_name(value: &Value) -> String {
    if let Value::Struct(s) = value {
        if let Some(name) = array_struct_element_type_name(&s.struct_name) {
            return name;
        }
    }
    "Any".to_string()
}

/// Extract the first (element) type parameter from an array wrapper type name,
/// honoring nested braces: `Array{Any, 1}` → `Any`, `Vector{Int64}` → `Int64`,
/// `Array{Complex{Float64}, 1}` → `Complex{Float64}`.
fn array_struct_element_type_name(struct_name: &str) -> Option<String> {
    let base = short_constructible_type_name(struct_name);
    let inner = base
        .strip_prefix("Array{")
        .or_else(|| base.strip_prefix("Vector{"))
        .or_else(|| base.strip_prefix("Matrix{"))?
        .strip_suffix('}')?;
    let mut depth = 0usize;
    for (i, c) in inner.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => return Some(inner[..i].trim().to_string()),
            _ => {}
        }
    }
    Some(inner.trim().to_string())
}

/// Resolve `value` to a `StructInstance` if it is a struct (by-ref or by-value),
/// for recursive reconstruction. Array wrappers are intentionally excluded here:
/// the caller handles them via `value_as_array` first.
fn as_struct<'a>(value: &'a Value, heap: &'a [StructInstance]) -> Option<&'a StructInstance> {
    match value {
        Value::StructRef(idx) => heap.get(*idx),
        Value::Struct(s) => Some(s),
        _ => None,
    }
}

/// Interpret `value` as an array (native array or a `Memory`-backed `Array`/
/// `Vector`/`Matrix` wrapper) for recursive reconstruction. Returns `None` for
/// scalars and non-wrapper structs (e.g. `Series`), which `linalg_value_to_array_value`
/// rejects with an `Err` rather than panicking.
fn value_as_array(value: &Value, heap: &[StructInstance]) -> Option<ArrayValue> {
    let looks_arrayish = crate::vm::value::is_native_array_value(value)
        || matches!(value, Value::StructRef(_) | Value::Struct(_));
    if !looks_arrayish {
        return None;
    }
    crate::vm::builtins_linalg::linalg_value_to_array_value(
        value.clone(),
        heap,
        "repl_persist",
        None,
    )
    .ok()
}

fn array_value_to_literal(arr: &ArrayValue) -> Option<Literal> {
    match arr.element_type() {
        ArrayElementType::F64 => {
            let mut data = Vec::with_capacity(arr.element_count());
            for idx in 0..arr.element_count() {
                match arr.get_linear(idx).ok()? {
                    Value::F64(v) => data.push(v),
                    _ => return None,
                }
            }
            Some(Literal::Array(data, arr.shape.clone()))
        }
        ArrayElementType::I64 => {
            let mut data = Vec::with_capacity(arr.element_count());
            for idx in 0..arr.element_count() {
                match arr.get_linear(idx).ok()? {
                    Value::I64(v) => data.push(v),
                    _ => return None,
                }
            }
            Some(Literal::ArrayI64(data, arr.shape.clone()))
        }
        ArrayElementType::Bool => {
            let mut data = Vec::with_capacity(arr.element_count());
            for idx in 0..arr.element_count() {
                match arr.get_linear(idx).ok()? {
                    Value::Bool(v) => data.push(v),
                    _ => return None,
                }
            }
            Some(Literal::ArrayBool(data, arr.shape.clone()))
        }
        // Complex, String, Char, Any, and StructRef arrays do not have literal
        // array variants yet. They remain available in REPLGlobals but are not
        // injected as Literal::Array*.
        _ => None,
    }
}

fn memory_value_to_literal(mem: &MemoryValue) -> Option<Literal> {
    let shape = vec![mem.len()];
    match mem.element_type() {
        ArrayElementType::F64 => {
            let mut data = Vec::with_capacity(mem.len());
            for idx in 1..=mem.len() {
                match mem.get(idx).ok()? {
                    Value::F64(v) => data.push(v),
                    _ => return None,
                }
            }
            Some(Literal::Array(data, shape))
        }
        ArrayElementType::I64 => {
            let mut data = Vec::with_capacity(mem.len());
            for idx in 1..=mem.len() {
                match mem.get(idx).ok()? {
                    Value::I64(v) => data.push(v),
                    _ => return None,
                }
            }
            Some(Literal::ArrayI64(data, shape))
        }
        ArrayElementType::Bool => {
            let mut data = Vec::with_capacity(mem.len());
            for idx in 1..=mem.len() {
                match mem.get(idx).ok()? {
                    Value::Bool(v) => data.push(v),
                    _ => return None,
                }
            }
            Some(Literal::ArrayBool(data, shape))
        }
        // Other Memory element types do not have Literal::Array* variants yet.
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
    if is_array_wrapper_name(struct_name) {
        return array_wrapper_struct_to_literal(instance);
    }

    let mut field_literals = Vec::with_capacity(instance.values.len());
    for value in &instance.values {
        // Delegate field conversion to value_to_literal() for consistent coverage.
        // value_to_literal() handles nested structs (incl. Complex, via the generic
        // Value::Struct arm — Issue #5163), Array (with element type preservation),
        // Memory, and all primitive types. Any type that value_to_literal() cannot
        // convert causes the whole struct to fail persistence.
        match value_to_literal(value) {
            Some(lit) => field_literals.push(lit),
            None => return None,
        }
    }
    Some(Literal::Struct(struct_name.to_string(), field_literals))
}

fn is_array_wrapper_name(name: &str) -> bool {
    let base = short_constructible_type_name(name);
    base == "Array" || base.starts_with("Array{")
}

fn short_constructible_type_name(name: &str) -> Cow<'_, str> {
    let Some(brace_idx) = name.find('{') else {
        return Cow::Borrowed(name.rsplit('.').next().unwrap_or(name));
    };

    let base = &name[..brace_idx];
    let params = &name[brace_idx..];
    let short_base = base.rsplit('.').next().unwrap_or(base);
    if short_base.len() == base.len() {
        Cow::Borrowed(name)
    } else {
        Cow::Owned(format!("{short_base}{params}"))
    }
}

fn array_wrapper_struct_to_literal(instance: &StructInstance) -> Option<Literal> {
    let storage = instance.values.first()?;
    let size = instance.values.get(1)?;
    let (shape, offset) = array_wrapper_shape_and_offset(size)?;
    let len: usize = shape.iter().product();

    let mut values = Vec::with_capacity(len);
    let element_type = if let Some(arr_ref) = crate::vm::value::native_array_value_ref(storage) {
        let arr = arr_ref.borrow();
        for linear in 0..len {
            values.push(arr.get_linear(offset - 1 + linear).ok()?);
        }
        arr.element_type()
    } else if let Value::Memory(mem_ref) = storage {
        let mem = mem_ref.borrow();
        for linear in 0..len {
            values.push(mem.get(offset + linear).ok()?);
        }
        mem.element_type.clone()
    } else {
        return None;
    };

    values_to_array_literal(values, shape, element_type)
}

fn values_to_array_literal(
    values: Vec<Value>,
    shape: Vec<usize>,
    element_type: ArrayElementType,
) -> Option<Literal> {
    match element_type {
        ArrayElementType::F64 => Some(Literal::Array(
            values
                .into_iter()
                .map(|value| match value {
                    Value::F64(v) => Some(v),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()?,
            shape,
        )),
        ArrayElementType::I64 => Some(Literal::ArrayI64(
            values
                .into_iter()
                .map(|value| match value {
                    Value::I64(v) => Some(v),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()?,
            shape,
        )),
        ArrayElementType::Bool => Some(Literal::ArrayBool(
            values
                .into_iter()
                .map(|value| match value {
                    Value::Bool(v) => Some(v),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()?,
            shape,
        )),
        _ => None,
    }
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

    // Issue #5163: a bare `Value::Struct` Complex must convert faithfully via the
    // generic struct path — preserving the real `struct_name` and field literal
    // types — rather than being collapsed to a hardcoded
    // `Complex{Float64}` / `Literal::Float` representation.

    #[test]
    fn test_value_to_literal_bare_complex_f64_struct_is_faithful() {
        use crate::vm::StructInstance;
        // Complex{Float64}(3.0, -4.0) as a bare struct value
        let value = Value::Struct(StructInstance::with_name(
            0,
            "Complex{Float64}".to_string(),
            vec![Value::F64(3.0), Value::F64(-4.0)],
        ));
        let result = value_to_literal(&value);
        let Some(Literal::Struct(name, fields)) = result else {
            panic!("Expected Literal::Struct for Complex{{Float64}}, got {result:?}");
        };
        assert_eq!(name, "Complex{Float64}");
        assert!(
            matches!(&fields[0], Literal::Float(re) if (re - 3.0).abs() < 1e-15),
            "re field should be Literal::Float(3.0), got {:?}",
            fields[0]
        );
        assert!(
            matches!(&fields[1], Literal::Float(im) if (im + 4.0).abs() < 1e-15),
            "im field should be Literal::Float(-4.0), got {:?}",
            fields[1]
        );
    }

    #[test]
    fn test_value_to_literal_bare_complex_int_struct_preserves_int_fields() {
        use crate::vm::StructInstance;
        // Complex{Int64}(5, 6) — fields must stay Literal::Int, NOT widen to Float
        let value = Value::Struct(StructInstance::with_name(
            0,
            "Complex{Int64}".to_string(),
            vec![Value::I64(5), Value::I64(6)],
        ));
        let result = value_to_literal(&value);
        let Some(Literal::Struct(name, fields)) = result else {
            panic!("Expected Literal::Struct for Complex{{Int64}}, got {result:?}");
        };
        assert_eq!(
            name, "Complex{Int64}",
            "struct_name must be preserved, not hardcoded to Complex{{Float64}}"
        );
        assert!(
            matches!(&fields[0], Literal::Int(5)),
            "re field should be Literal::Int(5), got {:?}",
            fields[0]
        );
        assert!(
            matches!(&fields[1], Literal::Int(6)),
            "im field should be Literal::Int(6), got {:?}",
            fields[1]
        );
    }

    #[test]
    fn test_value_to_literal_bare_complex_f32_struct_preserves_float32_fields() {
        use crate::vm::StructInstance;
        // Complex{Float32}(1.5f0, 2.5f0) — fields must stay Literal::Float32
        let value = Value::Struct(StructInstance::with_name(
            0,
            "Complex{Float32}".to_string(),
            vec![Value::F32(1.5_f32), Value::F32(2.5_f32)],
        ));
        let result = value_to_literal(&value);
        let Some(Literal::Struct(name, fields)) = result else {
            panic!("Expected Literal::Struct for Complex{{Float32}}, got {result:?}");
        };
        assert_eq!(
            name, "Complex{Float32}",
            "struct_name must be preserved, not hardcoded to Complex{{Float64}}"
        );
        assert!(
            matches!(&fields[0], Literal::Float32(re) if (re - 1.5_f32).abs() < 1e-6),
            "re field should be Literal::Float32(1.5), got {:?}",
            fields[0]
        );
        assert!(
            matches!(&fields[1], Literal::Float32(im) if (im - 2.5_f32).abs() < 1e-6),
            "im field should be Literal::Float32(2.5), got {:?}",
            fields[1]
        );
    }

    #[test]
    fn test_value_to_literal_bare_non_complex_struct_round_trips() {
        use crate::vm::StructInstance;
        // A bare non-Complex struct must also convert via the generic arm
        // (regression guard for the NamedTuple-field / Expr-arg paths that
        // pass whole structs through value_to_literal — Issue #5163 risk note).
        let value = Value::Struct(StructInstance::with_name(
            0,
            "Point".to_string(),
            vec![Value::I64(1), Value::I64(2)],
        ));
        let result = value_to_literal(&value);
        assert!(
            matches!(result, Some(Literal::Struct(ref name, ref fields))
                if name == "Point" && fields.len() == 2),
            "Expected Literal::Struct(Point, [..]), got {result:?}"
        );
    }

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
            matches!(
                value_to_literal(&Value::I16(1000)),
                Some(Literal::Int(1000))
            ),
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

    #[test]
    fn test_value_to_literal_reads_reshaped_array_logically() {
        use crate::vm::{new_array_ref, ArrayValue};

        let source = new_array_ref(ArrayValue::memory_first_from_i64(
            vec![1, 2, 3, 4],
            vec![2, 2],
        ));
        let reshaped = ArrayValue::reshaped_from_ref(&source, vec![4]).unwrap();
        source.borrow_mut().set(&[2, 2], Value::I64(40)).unwrap();

        let result = array_value_to_literal(&reshaped);

        assert!(
            matches!(result, Some(Literal::ArrayI64(ref data, ref shape))
                if data == &vec![1, 2, 3, 40] && shape == &vec![4]),
            "Expected logical reshaped array literal, got {:?}",
            result
        );
    }

    #[test]
    fn test_value_to_literal_memory_reads_storage_without_array_wrapper() {
        use crate::vm::value::{new_memory_ref, MemoryValue};

        let mut memory = MemoryValue::undef_typed(&ArrayElementType::I64, 3);
        memory.set(1, Value::I64(10)).unwrap();
        memory.set(2, Value::I64(20)).unwrap();
        memory.set(3, Value::I64(30)).unwrap();
        let mem = new_memory_ref(memory);
        let result = value_to_literal(&Value::Memory(mem));

        assert!(
            matches!(result, Some(Literal::ArrayI64(ref data, ref shape))
                if data == &vec![10, 20, 30] && shape == &vec![3]),
            "Expected Memory-backed literal array, got {:?}",
            result
        );
    }

    // Issue #3299: Regex persistence
    #[test]
    fn test_value_to_literal_regex_simple() {
        use crate::vm::value::RegexValue;
        let rv = RegexValue::new("hello", "").unwrap();
        let result = value_to_literal(&Value::Regex(Box::new(rv)));
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
        let result = value_to_literal(&Value::Regex(Box::new(rv)));
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
                Value::Regex(Box::new(RegexValue::new("test", "").expect("valid regex"))),
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
        // - Value::Memory: no Literal::Memory exists; REPLSession injects Memory
        //   globals via Memory{T}(undef, n) + setindex! reconstruction (Issue #4009)
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
        let instance = StructInstance::new(
            0,
            vec![Value::Enum {
                type_name: "Color".to_string(),
                value: 2,
            }],
        );
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
        let instance = StructInstance::new(
            0,
            vec![
                Value::I64(10),
                Value::Bool(false),
                Value::Char('a'),
                Value::Enum {
                    type_name: "Status".to_string(),
                    value: 0,
                },
            ],
        );
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
            matches!(value_to_literal(&Value::F64(1.5)), Some(Literal::Float(_))),
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
            matches!(value_to_literal(&Value::Nothing), Some(Literal::Nothing)),
            "Nothing must produce Literal::Nothing (Issue #3320)"
        );
        // Missing → Missing
        assert!(
            matches!(value_to_literal(&Value::Missing), Some(Literal::Missing)),
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
