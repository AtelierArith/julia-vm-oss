//! Expression compilation for CoreCompiler.
//!
//! This module contains expression-level compilation methods including
//! literal handling, binary/unary operations, function calls, and builtins.
//!
//! Submodules:
//! - `binary`: Binary operation compilation
//! - `builtin`: Builtin function compilation
//! - `call`: Function call compilation
//! - `collection`: Collection (array, dict) compilation
//! - `infer`: Type inference
//! - `struct_`: Struct compilation
//! - `unary`: Unary operation compilation

// `pub(crate)` so the Base-corpus parity gates in `compile::cache::tests` can
// referee the CoreType-native binary dispatch heuristics (Issue #6495, 6b-ii).
pub(crate) mod binary;
mod builtin;
mod builtin_array;
mod builtin_hof;
mod builtin_io;
mod builtin_math;
// builtin_set removed (Issue #3724): Set algebra now Pure Julia (base/set.jl)
mod builtin_string;
mod builtin_types;
// `pub(crate)` so the Base-corpus parity gates in `compile::cache::tests` can
// referee the CoreType-native call dispatch heuristics (Issue #6495, 6b-ii).
pub(crate) mod call;
mod coercion;
mod collection;
mod infer;
mod struct_;
mod unary;

pub(crate) use infer::{infer_array_element_type, infer_nested_array_literal_element_type};

use crate::ir::core::{Block, BuiltinOp, Expr, Literal, Stmt};
use crate::types::{JuliaType, TypeExpr};
use crate::vm::{ArrayElementType, ArrayValue, Instr, ModuleOperands, ValueType};
use half::f16;

use super::types::{err, CResult, CompileError};
use super::{
    get_math_constant_value, is_base_function, is_builtin_type_name, is_euler_name, is_pi_name,
    is_random_function, CoreCompiler,
};

fn is_julia_array_like_type(ty: &JuliaType) -> bool {
    matches!(
        ty,
        JuliaType::Array | JuliaType::VectorOf(_) | JuliaType::MatrixOf(_)
    ) || matches!(ty, JuliaType::Struct(name)
        if name == "Array"
            || name.starts_with("Array{")
            || name.starts_with("Vector{")
            || name.starts_with("Matrix{"))
}

fn is_dict_struct_name(name: &str) -> bool {
    // Split on `{` BEFORE stripping a module prefix: a parametric name like
    // `Dict{Symbolics.Num,Int64}` has a dot *inside* its type parameters, so
    // `rsplit('.')` on the whole string would wrongly yield `Num,Int64}`
    // (Issue #7173). Isolate the base (`Dict`) first, then drop any module
    // qualifier on it (`Base.Dict` -> `Dict`).
    let base = name.split('{').next().unwrap_or(name);
    let base = base.rsplit('.').next().unwrap_or(base);
    base == "Dict"
}

impl<'a> CoreCompiler<'a> {
    fn emit_builtin_irrational_singleton(&mut self, name: &str) -> Option<ValueType> {
        let symbol = if is_pi_name(name) {
            "π"
        } else if is_euler_name(name) {
            "ℯ"
        } else {
            return None;
        };
        let type_name = format!("Irrational{{:{}}}", symbol);
        let type_id = if let Some(info) = self.shared_ctx.struct_table.get(&type_name) {
            info.type_id
        } else {
            let type_arg = TypeExpr::RuntimeExpr(format!(":{}", symbol));
            self.shared_ctx
                .resolve_instantiation_with_type_expr("Irrational", &[type_arg])
                .ok()?
        };
        self.emit(Instr::NewStruct(type_id, 0));
        Some(ValueType::Struct(type_id))
    }

    fn module_private_type_object_name(&self, name: &str) -> Option<String> {
        if self.locals.contains_key(name) || name.contains('.') {
            return None;
        }
        let module_path = self.current_module_path.as_ref()?;
        let qualified = format!("{}.{}", module_path, name);
        (self.shared_ctx.struct_table.contains_key(&qualified)
            || self.shared_ctx.parametric_structs.contains_key(&qualified)
            || self.abstract_type_names.contains(&qualified)
            || self.shared_ctx.enum_types.contains_key(&qualified)
            || self.shared_ctx.is_primitive_type_name(&qualified))
        .then_some(qualified)
    }
}

fn block_opens_testset_scope(block: &Block) -> bool {
    block.stmts.iter().any(|stmt| match stmt {
        Stmt::Expr { expr, .. } => expr_opens_testset_scope(expr),
        _ => false,
    })
}

fn expr_opens_testset_scope(expr: &Expr) -> bool {
    match expr {
        Expr::Builtin {
            name: BuiltinOp::TestSetBegin,
            ..
        } => true,
        Expr::Call { function, .. } => function == "_testset_begin!",
        Expr::LetBlock { body, .. } => block_opens_testset_scope(body),
        _ => false,
    }
}

fn collect_declared_globals_in_testset_scope(
    block: &Block,
    out: &mut std::collections::HashSet<String>,
) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::Global { names, .. } => out.extend(names.iter().cloned()),
            Stmt::Expr { expr, .. } => collect_declared_globals_in_testset_expr(expr, out),
            Stmt::Block(block)
            | Stmt::Timed { body: block, .. }
            | Stmt::TestSet { body: block, .. }
            | Stmt::For { body: block, .. }
            | Stmt::ForEach { body: block, .. }
            | Stmt::ForEachTuple { body: block, .. }
            | Stmt::While { body: block, .. } => {
                collect_declared_globals_in_testset_scope(block, out);
            }
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_declared_globals_in_testset_scope(then_branch, out);
                if let Some(block) = else_branch {
                    collect_declared_globals_in_testset_scope(block, out);
                }
            }
            Stmt::Try {
                try_block,
                catch_block,
                else_block,
                finally_block,
                ..
            } => {
                collect_declared_globals_in_testset_scope(try_block, out);
                for block in [catch_block, else_block, finally_block]
                    .into_iter()
                    .flatten()
                {
                    collect_declared_globals_in_testset_scope(block, out);
                }
            }
            _ => {}
        }
    }
}

fn collect_declared_globals_in_testset_expr(
    expr: &Expr,
    out: &mut std::collections::HashSet<String>,
) {
    match expr {
        Expr::LetBlock { body, .. } => collect_declared_globals_in_testset_scope(body, out),
        Expr::Call { args, kwargs, .. } | Expr::ModuleCall { args, kwargs, .. } => {
            for arg in args {
                collect_declared_globals_in_testset_expr(arg, out);
            }
            for (_, value) in kwargs {
                collect_declared_globals_in_testset_expr(value, out);
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_declared_globals_in_testset_expr(left, out);
            collect_declared_globals_in_testset_expr(right, out);
        }
        Expr::UnaryOp { operand, .. } => collect_declared_globals_in_testset_expr(operand, out),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            collect_declared_globals_in_testset_expr(condition, out);
            collect_declared_globals_in_testset_expr(then_expr, out);
            collect_declared_globals_in_testset_expr(else_expr, out);
        }
        Expr::TupleLiteral { elements, .. } | Expr::ArrayLiteral { elements, .. } => {
            for elem in elements {
                collect_declared_globals_in_testset_expr(elem, out);
            }
        }
        _ => {}
    }
}

fn array_literal_element_ranks(elements: &[Expr]) -> Option<Vec<usize>> {
    elements
        .iter()
        .map(|element| match element {
            Expr::ArrayLiteral { shape, .. } => Some(shape.len()),
            _ => None,
        })
        .collect()
}

fn tuple_field_array_element_type(value_type: &ValueType) -> ArrayElementType {
    match value_type {
        ValueType::I8 => ArrayElementType::I8,
        ValueType::I16 => ArrayElementType::I16,
        ValueType::I32 => ArrayElementType::I32,
        ValueType::I64 => ArrayElementType::I64,
        ValueType::U8 => ArrayElementType::U8,
        ValueType::U16 => ArrayElementType::U16,
        ValueType::U32 => ArrayElementType::U32,
        ValueType::U64 => ArrayElementType::U64,
        ValueType::F32 => ArrayElementType::F32,
        ValueType::F64 => ArrayElementType::F64,
        ValueType::Bool => ArrayElementType::Bool,
        ValueType::Str => ArrayElementType::String,
        ValueType::Char => ArrayElementType::Char,
        ValueType::Symbol => ArrayElementType::Symbol,
        _ => ArrayElementType::Any,
    }
}

/// Preserve a concrete *parametric* element type written as a `T[]` literal
/// (`UnitRange{Int64}[]`, `Vector{Int}[]`, ...) instead of widening it to `Any`
/// (Issue #6768). Returns `Some(Abstract(name))` when `type_name` is a
/// parameterized type whose every component is a type name (uppercase-initial),
/// i.e. it carries no free type variable; otherwise `None` so the caller keeps
/// the legacy `Any` fallback.
///
/// Concrete-storage parametric eltypes (`Complex{Float64}`) and registered
/// structs are handled by earlier arms before this fallback is consulted.
pub(in crate::compile::expr) fn concrete_parametric_element_type_from_name(
    type_name: &str,
) -> Option<ArrayElementType> {
    // Must be a parametric form `Base{...}` with a non-empty parameter list.
    let open = type_name.find('{')?;
    if !type_name.ends_with('}') || open + 1 >= type_name.len() - 1 {
        return None;
    }
    // Every identifier token (base + each parameter component) must look like a
    // concrete type name (starts uppercase), so we never preserve a free type
    // variable such as `UnitRange{T}` written inside a `where` body.
    let mut ident = String::new();
    for ch in type_name.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            ident.push(ch);
        } else {
            if !is_concrete_type_ident(&ident) {
                return None;
            }
            ident.clear();
        }
    }
    if !is_concrete_type_ident(&ident) {
        return None;
    }
    Some(ArrayElementType::Abstract(type_name.to_string()))
}

/// An identifier inside a parametric type name is "concrete" when it is empty
/// (a separator boundary already validated by its neighbours) or begins with an
/// uppercase letter (a type name, not a lowercase type variable).
fn is_concrete_type_ident(ident: &str) -> bool {
    ident
        .chars()
        .next()
        .is_none_or(|c| c.is_uppercase() || c.is_ascii_digit())
}

impl CoreCompiler<'_> {
    fn tuple_literal_array_element_type(&mut self, elements: &[Expr]) -> Option<ArrayElementType> {
        let mut tuple_fields: Option<Vec<ArrayElementType>> = None;
        for element in elements {
            let Expr::TupleLiteral {
                elements: fields, ..
            } = element
            else {
                return None;
            };
            let field_types: Vec<ArrayElementType> = fields
                .iter()
                .map(|field| tuple_field_array_element_type(&self.infer_expr_type(field)))
                .collect();
            match &tuple_fields {
                Some(existing) if existing != &field_types => return None,
                Some(_) => {}
                None => tuple_fields = Some(field_types),
            }
        }
        tuple_fields.map(ArrayElementType::TupleOf)
    }

    pub(in crate::compile) fn is_array_wrapper_value_type(&self, ty: &ValueType) -> bool {
        matches!(ty, ValueType::Struct(type_id)
        if self.shared_ctx.get_struct_name(*type_id).is_some_and(|name| {
            name == "Array"
                || name.starts_with("Array{")
                || name.starts_with("Vector{")
                || name.starts_with("Matrix{")
        }))
    }

    pub(in crate::compile::expr) fn emit_array_wrapper_memory_start(
        &mut self,
        elem_type: ArrayElementType,
        len: usize,
    ) {
        // Build the backing `Memory{T}` directly. The finalize step
        // (`emit_array_wrapper_from_memory_on_stack`) wraps it into the
        // `Array{T,N}` natively, so we no longer push the `Array` `DataType`
        // that the old pure-Julia `wrap(::Type{Array}, ...)` call consumed
        // (Issue #6846).
        self.emit(Instr::NewMemory(elem_type, len));
    }

    pub(in crate::compile::expr) fn emit_array_wrapper_from_memory_on_stack(
        &mut self,
        shape: &[usize],
    ) {
        // Wrap the `Memory{T}` on top of the stack into the `Array{T,N}`
        // wrapper with a native `FinalizeArray` instead of a per-literal
        // pure-Julia `wrap(::Type{Array}, mem, dims)` call. `wrap` spun up
        // ~5 Julia frames (`wrap` → `_array_wrap_check` → `memoryref` →
        // `_array_construct` → `Array{T,N}(ref, dims)`) for every array
        // literal, which dominated tight allocation loops such as
        // `(x, y) -> sinc(norm([x, y]))` over a grid (Issue #6846). The
        // `FinalizeArray` handler reconstructs the exact same wrapper from the
        // `Memory` build buffer (shared with the comprehension build path,
        // Issue #6807) with no Julia frame.
        self.emit(Instr::FinalizeArray(shape.to_vec()));
    }

    pub(in crate::compile::expr) fn emit_empty_array_wrapper(
        &mut self,
        elem_type: ArrayElementType,
        shape: &[usize],
    ) {
        let len = shape.iter().product();
        self.emit_array_wrapper_memory_start(elem_type, len);
        self.emit_array_wrapper_from_memory_on_stack(shape);
    }

    /// Compile one element of an inline `Complex{Float64}` / `Complex{Float32}`
    /// array literal, coercing it to the array's storage type (Issue #6867).
    ///
    /// The element may be:
    /// - already the exact inline complex type (`target`) → pushed as-is;
    /// - a `Complex{...}` value of a different parameter (e.g. `Complex{Float32}`
    ///   into a `Complex{Float64}` array, from a `Complex×Real` promotion) →
    ///   rebuilt via `target_name(real(z), imag(z))`;
    /// - a real numeric (`Float64`, `Int64`, ...) → widened via
    ///   `target_name(x, 0)`, mirroring `promote_type(Complex{T}, Real)`.
    fn compile_complex_array_element(
        &mut self,
        elem: &Expr,
        elem_type: &ValueType,
        target: ValueType,
        target_name: &str,
    ) -> CResult<()> {
        // Exact inline complex value: store directly.
        if *elem_type == target {
            self.compile_expr(elem)?;
            return Ok(());
        }
        // A struct-backed `Complex{T}` whose name matches the target storage is
        // representation-compatible; store directly (existing fast path).
        if let ValueType::Struct(id) = elem_type {
            if self.shared_ctx.get_struct_name(*id).as_deref() == Some(target_name) {
                self.compile_expr(elem)?;
                return Ok(());
            }
        }

        let span = elem.span();
        let is_complex_elem = matches!(elem_type, ValueType::ComplexF32 | ValueType::ComplexF64)
            || self.is_struct_type_of(elem_type.clone(), "Complex");

        let args = if is_complex_elem {
            // Convert a differently-parameterized Complex via real/imag parts.
            let real_call = Expr::Call {
                function: "real".to_string(),
                args: vec![elem.clone()],
                kwargs: Vec::new(),
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span,
            };
            let imag_call = Expr::Call {
                function: "imag".to_string(),
                args: vec![elem.clone()],
                kwargs: Vec::new(),
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span,
            };
            vec![real_call, imag_call]
        } else {
            // Real numeric element: imaginary part is zero.
            vec![elem.clone(), Expr::Literal(Literal::Int(0), span)]
        };

        let complex_call = Expr::Call {
            function: target_name.to_string(),
            args,
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        };
        self.compile_expr_as(&complex_call, target)
    }

    pub(super) fn compile_expr(&mut self, expr: &Expr) -> CResult<ValueType> {
        match expr {
            Expr::Literal(lit, span) => match lit {
                Literal::Int(v) => {
                    self.emit(Instr::PushI64(*v));
                    Ok(ValueType::I64)
                }
                Literal::Int128(v) => {
                    self.emit(Instr::PushI128(Box::new(*v)));
                    Ok(ValueType::I128)
                }
                Literal::BigInt(s) => {
                    self.emit(Instr::PushBigInt(s.clone()));
                    Ok(ValueType::BigInt)
                }
                Literal::BigFloat(s) => {
                    self.emit(Instr::PushBigFloat(s.clone()));
                    Ok(ValueType::BigFloat)
                }
                Literal::Float(v) => {
                    self.emit(Instr::PushF64(*v));
                    Ok(ValueType::F64)
                }
                Literal::Float32(v) => {
                    self.emit(Instr::PushF32(*v));
                    Ok(ValueType::F32)
                }
                Literal::Float16(v) => {
                    self.emit(Instr::PushF16(*v));
                    Ok(ValueType::F16)
                }
                Literal::Bool(b) => {
                    self.emit(Instr::PushBool(*b));
                    Ok(ValueType::Bool)
                }
                Literal::Str(s) => {
                    self.emit(Instr::PushStr(s.clone()));
                    Ok(ValueType::Str)
                }
                Literal::Char(c) => {
                    self.emit(Instr::PushChar(*c));
                    Ok(ValueType::Char)
                }
                Literal::Nothing => {
                    self.emit(Instr::PushNothing);
                    Ok(ValueType::Nothing)
                }
                Literal::Missing => {
                    self.emit(Instr::PushMissing);
                    Ok(ValueType::Missing)
                }
                Literal::Array(data, shape) => {
                    self.emit(Instr::PushArrayValue(Box::new(
                        ArrayValue::memory_first_from_f64(data.clone(), shape.clone()),
                    )));
                    Ok(ValueType::ArrayOf(ArrayElementType::F64, None))
                }
                Literal::ArrayI64(data, shape) => {
                    self.emit(Instr::PushArrayValue(Box::new(
                        ArrayValue::memory_first_from_i64(data.clone(), shape.clone()),
                    )));
                    Ok(ValueType::ArrayOf(ArrayElementType::I64, None))
                }
                Literal::ArrayBool(data, shape) => {
                    self.emit(Instr::PushArrayValue(Box::new(
                        ArrayValue::memory_first_from_bool(data.clone(), shape.clone()),
                    )));
                    Ok(ValueType::ArrayOf(ArrayElementType::Bool, None))
                }
                Literal::Struct(type_name, field_literals) => {
                    // Look up struct info by name
                    let struct_info =
                        self.shared_ctx.struct_table.get(type_name).ok_or_else(|| {
                            CompileError::Msg(format!("Unknown struct type: {}", type_name))
                        })?;
                    let type_id = struct_info.type_id;
                    let expected_field_count = struct_info.fields.len();
                    let field_types: Vec<ValueType> = struct_info
                        .fields
                        .iter()
                        .map(|(_, ty)| ty.clone())
                        .collect();

                    if field_literals.len() != expected_field_count {
                        return err(format!(
                            "Struct {} expects {} fields, got {}",
                            type_name,
                            expected_field_count,
                            field_literals.len()
                        ));
                    }

                    // Compile each field literal with the expected type
                    for (literal, expected_ty) in field_literals.iter().zip(field_types.iter()) {
                        let literal_expr = Expr::Literal(literal.clone(), *span);
                        self.compile_expr_as(&literal_expr, expected_ty.clone())?;
                    }

                    // Emit NewStruct instruction
                    self.emit(Instr::NewStruct(type_id, field_literals.len()));
                    Ok(ValueType::Struct(type_id))
                }
                Literal::Module(name) => {
                    let export_key = self
                        .current_module_path
                        .as_deref()
                        .filter(|path| {
                            path.rsplit('.').next() == Some(name.as_str())
                                && self.module_exports.contains_key(*path)
                        })
                        .unwrap_or(name.as_str());
                    let module_name = export_key.to_string();
                    let exports = self
                        .module_exports
                        .get(export_key)
                        .map(|set| {
                            let mut exports: Vec<String> = set.iter().cloned().collect();
                            exports.sort();
                            exports
                        })
                        .unwrap_or_default();
                    self.emit(Instr::PushModule(Box::new(ModuleOperands {
                        name: module_name,
                        exports,
                        publics: vec![],
                    })));
                    Ok(ValueType::Module)
                }
                Literal::DataType(name) => {
                    self.emit(Instr::PushDataType(name.clone()));
                    Ok(ValueType::DataType)
                }
                Literal::Undef => {
                    // Undef is used for required keyword arguments (no default value)
                    self.emit(Instr::PushUndef);
                    Ok(ValueType::Any)
                }
                // Metaprogramming literals (for REPL persistence) and
                // macro-injected `QuoteNode(:sym)` arguments (Issue #7163). The
                // emitted `PushSymbol` produces a genuine `Value::Symbol`, so the
                // static type must be `Symbol` (not `Any`) to match a `::Symbol`
                // field/parameter slot — otherwise the constructor field coercion
                // sees `Any` and errors with "Cannot convert Any to Symbol".
                // Mirrors the source-level `:sym` path (`QuoteLiteral(SymbolNew)`,
                // which already reports `ValueType::Symbol`) and the literal-type
                // inference functions (`infer_expr_type`, `literal_rhs_value_type`,
                // `infer_default_type`).
                Literal::Symbol(name) => {
                    self.emit(Instr::PushSymbol(name.clone()));
                    Ok(ValueType::Symbol)
                }
                Literal::Expr { head, args } => {
                    // Compile each arg literal first (they will be pushed on stack)
                    for arg in args {
                        let arg_expr = Expr::Literal(arg.clone(), *span);
                        self.compile_expr(&arg_expr)?;
                    }
                    // Emit CreateExpr to pop args and create Expr value
                    self.emit(Instr::CreateExpr {
                        head: head.clone(),
                        arg_count: args.len(),
                    });
                    Ok(ValueType::Any)
                }
                Literal::QuoteNode(inner) => {
                    // Compile the inner literal
                    let inner_expr = Expr::Literal(inner.as_ref().clone(), *span);
                    self.compile_expr(&inner_expr)?;
                    // Wrap in QuoteNode
                    self.emit(Instr::CreateQuoteNode);
                    Ok(ValueType::Any)
                }
                Literal::LineNumberNode { line, file } => {
                    self.emit(Instr::PushLineNumberNode {
                        line: *line,
                        file: file.clone(),
                    });
                    Ok(ValueType::Any)
                }
                Literal::Regex { pattern, flags } => {
                    self.emit(Instr::PushRegex {
                        pattern: pattern.clone(),
                        flags: flags.clone(),
                    });
                    Ok(ValueType::Regex)
                }
                Literal::Enum { type_name, value } => {
                    self.emit(Instr::PushEnum {
                        type_name: type_name.clone(),
                        value: *value,
                    });
                    Ok(ValueType::Enum)
                }
            },
            Expr::Var(name, _) => {
                if name == "nothing" && !self.locals.contains_key(name) {
                    self.emit(Instr::PushNothing);
                    return Ok(ValueType::Nothing);
                }

                // A name declared `global x` is unambiguously a variable read of
                // the module-level binding. Resolve it through `load_local`
                // (which emits a global-aware `LoadAny`) and skip the local
                // strict-undefined check, since the binding lives in frame 0 and
                // may not exist in `locals`/`global_types` at compile time
                // (Issues #5548, #5549).
                if self.declared_globals.contains(name) {
                    self.load_local(name)?;
                    return Ok(ValueType::Any);
                }

                // A captured closure variable shadows any same-named Base
                // function, type name, or module alias — exactly as a plain
                // local does. Resolve it through `load_local` (which emits
                // `LoadCaptured`) BEFORE the Base-function / type-name checks
                // below; otherwise a captured accumulator whose name collides
                // with a `Base` function (e.g. `count`, `sum`) would compile to
                // `PushFunction("count")` and the closure body would operate on
                // the `Base` function value instead of the captured local
                // (Issue #7619).
                if self.captured_vars.contains(name) && !self.locals.contains_key(name) {
                    self.load_local(name)?;
                    return Ok(ValueType::Any);
                }

                // Check if this is a type parameter from a where clause
                // Type parameters are resolved at runtime
                if self.current_type_param_index.contains_key(name.as_str()) {
                    // Check if this is a Val{N} type parameter - these are values (int/bool/symbol), not types
                    if self.val_type_params.contains(name)
                        || self.val_bool_params.contains(name)
                        || self.val_symbol_params.contains(name)
                    {
                        // Val type parameters are stored in specialized maps at runtime
                        // Use LoadAny to check all possible storages (i64, bool, symbol)
                        self.emit(Instr::LoadAny(name.clone()));
                        return Ok(ValueType::Any);
                    }
                    // Regular type parameters are resolved via LoadTypeBinding
                    self.emit(Instr::LoadTypeBinding(name.clone()));
                    return Ok(ValueType::DataType);
                }

                // Handle pi/π, NaN, Inf constants (always available without imports).
                // Built-in irrational singletons are also recorded in global_const_structs;
                // preserve those bindings when present instead of lowering the variable
                // reference directly to a Float64 literal (Issue #8481).
                if !self.locals.contains_key(name) {
                    if let Some(ty) = self.emit_builtin_irrational_singleton(name) {
                        return Ok(ty);
                    }
                    if is_pi_name(name) {
                        self.emit(Instr::PushF64(std::f64::consts::PI));
                        return Ok(ValueType::F64);
                    }
                    if is_euler_name(name) {
                        self.emit(Instr::PushF64(std::f64::consts::E));
                        return Ok(ValueType::F64);
                    }
                    if name == "NaN" {
                        self.emit(Instr::PushF64(f64::NAN));
                        return Ok(ValueType::F64);
                    }
                    if name == "Inf" {
                        self.emit(Instr::PushF64(f64::INFINITY));
                        return Ok(ValueType::F64);
                    }
                    // Handle Float32 special values
                    if name == "Inf32" {
                        self.emit(Instr::PushF32(f32::INFINITY));
                        return Ok(ValueType::F32);
                    }
                    if name == "NaN32" {
                        self.emit(Instr::PushF32(f32::NAN));
                        return Ok(ValueType::F32);
                    }
                    // Handle Float16 special values
                    if name == "Inf16" {
                        self.emit(Instr::PushF16(f16::INFINITY));
                        return Ok(ValueType::F16);
                    }
                    if name == "NaN16" {
                        self.emit(Instr::PushF16(f16::NAN));
                        return Ok(ValueType::F16);
                    }
                    // Handle explicit Float64 special value aliases
                    if name == "Inf64" {
                        self.emit(Instr::PushF64(f64::INFINITY));
                        return Ok(ValueType::F64);
                    }
                    if name == "NaN64" {
                        self.emit(Instr::PushF64(f64::NAN));
                        return Ok(ValueType::F64);
                    }
                    // Handle Julia global constants: ARGS, PROGRAM_FILE
                    // Note: VERSION is defined in version.jl as a VersionNumber struct,
                    // not handled as a special case here.
                    if name == "ARGS" {
                        // ARGS is an empty String array (command-line args not passed through)
                        self.emit(Instr::NewArrayTyped(ArrayElementType::String, 0));
                        self.emit(Instr::FinalizeArrayTyped(vec![0]));
                        return Ok(ValueType::ArrayOf(ArrayElementType::String, None));
                    }
                    if name == "PROGRAM_FILE" {
                        // PROGRAM_FILE is empty string when in REPL/embedded mode
                        self.emit(Instr::PushStr(String::new()));
                        return Ok(ValueType::Str);
                    }
                    if name == "ENDIAN_BOM" {
                        // ENDIAN_BOM: 32-bit byte-order-mark indicating native byte order
                        // Little-endian: 0x04030201, Big-endian: 0x01020304
                        // Most modern systems are little-endian
                        #[cfg(target_endian = "little")]
                        let bom: i64 = 0x04030201;
                        #[cfg(target_endian = "big")]
                        let bom: i64 = 0x01020304;
                        self.emit(Instr::PushI64(bom));
                        return Ok(ValueType::I64);
                    }
                    // Standard IO streams
                    if name == "stdout" {
                        self.emit(Instr::PushStdout);
                        return Ok(ValueType::IO);
                    }
                    if name == "stderr" {
                        self.emit(Instr::PushStderr);
                        return Ok(ValueType::IO);
                    }
                    if name == "stdin" {
                        self.emit(Instr::PushStdin);
                        return Ok(ValueType::IO);
                    }
                    if name == "devnull" {
                        self.emit(Instr::PushDevnull);
                        return Ok(ValueType::IO);
                    }
                    // C_NULL: Null pointer constant (Ptr{Cvoid}(0))
                    if name == "C_NULL" {
                        self.emit(Instr::PushCNull);
                        return Ok(ValueType::I64);
                    }
                    // DEPOT_PATH: Array of depot paths (empty in SubsetJuliaVM)
                    if name == "DEPOT_PATH" {
                        self.emit(Instr::NewArrayTyped(ArrayElementType::String, 0));
                        self.emit(Instr::FinalizeArrayTyped(vec![0]));
                        return Ok(ValueType::ArrayOf(ArrayElementType::String, None));
                    }
                    // LOAD_PATH: Array of load paths (empty in SubsetJuliaVM)
                    if name == "LOAD_PATH" {
                        self.emit(Instr::NewArrayTyped(ArrayElementType::String, 0));
                        self.emit(Instr::FinalizeArrayTyped(vec![0]));
                        return Ok(ValueType::ArrayOf(ArrayElementType::String, None));
                    }
                    // ENV: Environment variable dictionary (read-only
                    // Dict{String,String}). PushEnv supplies the raw OS pairs as
                    // a tuple of `(key, value)` 2-tuples; the pure-Julia
                    // `_env_from_pairs` helper builds the `Dict{String,String}`
                    // struct via the ordinary constructor, so ENV is a pure
                    // `Dict{K,V}` StructRef with no `Value::Dict` carrier
                    // (Issue #6731).
                    if name == "ENV" {
                        self.emit(Instr::PushEnv);
                        let candidates = self.runtime_candidates_for_names(&["_env_from_pairs"], 1);
                        if let Some(&fallback) = candidates.first() {
                            self.emit(Instr::CallTypedDispatch(
                                "_env_from_pairs".to_string(),
                                1,
                                fallback,
                                candidates,
                            ));
                        }
                        return Ok(ValueType::Any);
                    }
                }
                // Handle type names - push as DataType values for proper Julia semantics
                // Type names like Int64, Float64 are first-class values of type DataType
                if !self.locals.contains_key(name) {
                    // Check if it's a type alias (const MyInt = Int64)
                    // Resolve the alias to its target type
                    if let Some(target_type) = self.resolve_visible_type_alias(name) {
                        self.emit(Instr::PushDataType(target_type));
                        return Ok(ValueType::DataType);
                    }
                    // Check if it's a built-in type name
                    if is_builtin_type_name(name) {
                        self.emit(Instr::PushDataType(name.to_string()));
                        return Ok(ValueType::DataType);
                    }
                    // Check if it's a user type object. Bare type names inside a
                    // module must resolve to that module's binding, not the
                    // unqualified short-name cache entry, so returned type
                    // objects compare/subtype against `Module.T` correctly
                    // (Issue #8410).
                    if let Some(type_name) = self.resolve_visible_type_object_name(name) {
                        self.emit(Instr::PushDataType(type_name));
                        return Ok(ValueType::DataType);
                    }
                }
                if self
                    .locals
                    .get(name)
                    .is_none_or(|ty| matches!(ty, ValueType::Module))
                {
                    if let Some(module_path) = self.nested_module_path_in_current_module(name) {
                        let exports = self
                            .module_exports
                            .get(module_path.as_str())
                            .map(|set| set.iter().cloned().collect())
                            .unwrap_or_default();
                        self.emit(Instr::PushModule(Box::new(ModuleOperands {
                            name: module_path,
                            exports,
                            publics: vec![],
                        })));
                        return Ok(ValueType::Module);
                    }
                    if let Some(module_path) = self.module_aliases.get(name).cloned() {
                        let exports = self
                            .module_exports
                            .get(module_path.as_str())
                            .map(|set| set.iter().cloned().collect())
                            .unwrap_or_default();
                        self.emit(Instr::PushModule(Box::new(ModuleOperands {
                            name: module_path,
                            exports,
                            publics: vec![],
                        })));
                        return Ok(ValueType::Module);
                    }
                }
                if !self.locals.contains_key(name) {
                    for using_module in self.visible_using_modules_for_name(name) {
                        if self.module_exports.contains_key(name)
                            || self.module_functions.contains_key(name)
                        {
                            let exports = self
                                .module_exports
                                .get(name)
                                .map(|set| {
                                    let mut exports: Vec<String> = set.iter().cloned().collect();
                                    exports.sort();
                                    exports
                                })
                                .unwrap_or_default();
                            self.emit(Instr::PushModule(Box::new(ModuleOperands {
                                name: name.clone(),
                                exports,
                                publics: vec![],
                            })));
                            return Ok(ValueType::Module);
                        }
                        let is_module_constant = self
                            .module_constants
                            .get(using_module.as_str())
                            .is_some_and(|constants| constants.contains(name));
                        let qualified = format!("{}.{}", using_module, name);
                        let is_function = self.method_tables.contains_key(name)
                            || self.method_tables.contains_key(&qualified)
                            || is_base_function(name);
                        if is_module_constant || !is_function {
                            self.emit(Instr::LoadGlobalAny(qualified));
                            return Ok(ValueType::Any);
                        }
                    }
                }
                // Resolve bare function names to function objects when not a local variable
                if !self.locals.contains_key(name) {
                    if self.method_tables.contains_key(name)
                        && !self.hidden_user_globals.contains(name)
                    {
                        if !self.imported_functions.contains(name) {
                            return err(format!(
                                "function '{}' is not imported. Use 'using ModuleName' or 'using ModuleName: {}' to import it, or use 'ModuleName.{}()' for qualified access.",
                                name, name, name
                            ));
                        }
                        self.emit_function_value(name);
                        return Ok(ValueType::Function);
                    }
                    if is_base_function(name) {
                        self.emit_function_value(name);
                        return Ok(ValueType::Function);
                    }
                    if self.usings.contains("Random") && is_random_function(name) {
                        self.emit(Instr::PushFunction(format!("Random.{}", name)));
                        return Ok(ValueType::Function);
                    }
                    // Handle MathConstants when imported via `using Base.MathConstants`
                    if self.usings.contains("Base.MathConstants") {
                        if let Some(value) = get_math_constant_value(name) {
                            self.emit(Instr::PushF64(value));
                            return Ok(ValueType::F64);
                        }
                    }
                }
                // Julia allows unresolved references to remain in compiled code
                // and raises UndefVarError only if execution reaches the load.
                // Macro-expanded code from MacroTools relies on that behavior:
                // rejecting the reference here prevents Julia-valid expansions
                // from compiling. Let load_local emit a generic LoadAny, whose VM
                // path already raises UndefVarError when no local/global/type
                // binding exists (Issue #7556).
                let in_locals = self.locals.contains_key(name);

                // If this is a const struct that can be inlined, emit NewStruct instead of load
                if !in_locals {
                    if let Some((_struct_name, type_id, field_count)) = self
                        .shared_ctx
                        .global_const_structs
                        .get(name)
                        .map(|(s, t, f)| (s.clone(), *t, *f))
                    {
                        // Inline the struct constructor: emit NewStruct(type_id, field_count)
                        // For empty structs like `const M = MyType()`, this creates a new instance
                        self.emit(Instr::NewStruct(type_id, field_count));
                        return Ok(ValueType::Struct(type_id));
                    }
                }

                // Bare abstract-numeric params (`x::Real`, `x::Number`, ...) load via
                // `LoadAny` (see `load_local`) because the runtime value keeps its
                // concrete type (Int8/Int64/Float32/...). Their `locals` slot, however,
                // is the annotation's widened `ValueType::F64`/`I64`, so reporting that
                // here would make a direct return `f(x::Real)=x` emit `ReturnF64`, which
                // coerces the concrete runtime value (e.g. `Int64(3)` → `Float64(3.0)`)
                // and the typed caller slot then rejects/mistypes it. Report `Any` to
                // match the `LoadAny` representation, so the direct return uses
                // `ReturnAny` and preserves the concrete runtime type — symmetric with
                // the `infer_julia_type` (#5076/#5169) and `infer_expr_type`
                // (#5167 part 2 / #5243) guards (Issue #5242).
                if self.abstract_numeric_params.contains(name) {
                    self.load_local(name)?;
                    return Ok(ValueType::Any);
                }

                // Prefer local type, fall back to global type, then default to Any
                // (not I64, to ensure dynamic dispatch for unknown types)
                let ty = self
                    .locals
                    .get(name)
                    .cloned()
                    .or_else(|| self.shared_ctx.global_types.get(name).cloned())
                    .unwrap_or(ValueType::Any);
                self.load_local(name)?;
                Ok(ty)
            }
            Expr::BinaryOp {
                op, left, right, ..
            } => self.compile_binary_op(op, left, right),
            Expr::UnaryOp { op, operand, span } => self.compile_unary_op(op, operand, *span),
            Expr::Call {
                function,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                ..
            } => self.compile_call(function, args, kwargs, splat_mask, kwargs_splat_mask),
            Expr::Builtin { name, args, .. } => {
                // Base functions are never implicitly shadowed.
                // To extend Base functions, use Base.func(x::T) = ... syntax.
                self.compile_builtin(name, args)
            }
            Expr::ArrayLiteral {
                elements, shape, ..
            } => {
                // Infer types of all elements
                let elem_types: Vec<ValueType> = elements
                    .iter()
                    .map(|elem| self.infer_expr_type(elem))
                    .collect();

                // Determine array element type based on element types. Nested
                // array literals need their inner rank to distinguish
                // Vector{T} from Matrix{T}; ValueType::ArrayOf alone cannot
                // carry that information (Issue #6227).
                let nested_ranks = array_literal_element_ranks(elements);
                let array_elem_type = nested_ranks
                    .as_deref()
                    .and_then(|ranks| infer_nested_array_literal_element_type(&elem_types, ranks))
                    .or_else(|| self.tuple_literal_array_element_type(elements))
                    .unwrap_or_else(|| {
                        infer_array_element_type(
                            &elem_types,
                            |type_id| self.shared_ctx.get_struct_name(type_id),
                            |name| {
                                self.shared_ctx
                                    .struct_table
                                    .get(name)
                                    .map(|info| info.type_id)
                            },
                        )
                        .0
                    });

                match array_elem_type {
                    ArrayElementType::I64 => {
                        // All integer elements: use Memory{Int64} + Array wrapper.
                        self.emit_array_wrapper_memory_start(ArrayElementType::I64, elements.len());
                        for (index, elem) in elements.iter().enumerate() {
                            self.emit(Instr::PushI64((index + 1) as i64));
                            self.compile_expr_as(elem, ValueType::I64)?;
                            self.emit(Instr::MemorySet);
                        }
                        self.emit_array_wrapper_from_memory_on_stack(shape);
                        Ok(ValueType::ArrayOf(ArrayElementType::I64, None))
                    }
                    ArrayElementType::F64 => {
                        // Numeric elements (with at least one float): use Memory{Float64}.
                        self.emit_array_wrapper_memory_start(ArrayElementType::F64, elements.len());
                        for (index, elem) in elements.iter().enumerate() {
                            self.emit(Instr::PushI64((index + 1) as i64));
                            self.compile_expr_as(elem, ValueType::F64)?;
                            self.emit(Instr::MemorySet);
                        }
                        self.emit_array_wrapper_from_memory_on_stack(shape);
                        Ok(ValueType::ArrayOf(ArrayElementType::F64, None))
                    }
                    ArrayElementType::ComplexF64 => {
                        self.emit_array_wrapper_memory_start(
                            ArrayElementType::ComplexF64,
                            elements.len(),
                        );
                        for (index, (elem, elem_type)) in
                            elements.iter().zip(elem_types.iter()).enumerate()
                        {
                            self.emit(Instr::PushI64((index + 1) as i64));
                            self.compile_complex_array_element(
                                elem,
                                elem_type,
                                ValueType::ComplexF64,
                                "Complex{Float64}",
                            )?;
                            self.emit(Instr::MemorySet);
                        }
                        self.emit_array_wrapper_from_memory_on_stack(shape);
                        Ok(ValueType::ArrayOf(ArrayElementType::ComplexF64, None))
                    }
                    ArrayElementType::ComplexF32 => {
                        self.emit_array_wrapper_memory_start(
                            ArrayElementType::ComplexF32,
                            elements.len(),
                        );
                        for (index, (elem, elem_type)) in
                            elements.iter().zip(elem_types.iter()).enumerate()
                        {
                            self.emit(Instr::PushI64((index + 1) as i64));
                            self.compile_complex_array_element(
                                elem,
                                elem_type,
                                ValueType::ComplexF32,
                                "Complex{Float32}",
                            )?;
                            self.emit(Instr::MemorySet);
                        }
                        self.emit_array_wrapper_from_memory_on_stack(shape);
                        Ok(ValueType::ArrayOf(ArrayElementType::ComplexF32, None))
                    }
                    ArrayElementType::StructOf(type_id) => {
                        // Struct array - check if we need type promotion (e.g., Int -> Rational, Int -> Complex)
                        let struct_name = self.shared_ctx.get_struct_name(type_id);
                        let is_rational = struct_name
                            .as_ref()
                            .map(|n| crate::vm::value::is_rational_type_name(n))
                            .unwrap_or(false);
                        let is_complex = struct_name
                            .as_ref()
                            .map(|n| n.starts_with("Complex"))
                            .unwrap_or(false);
                        // Get the target Complex type name for constructor calls
                        let complex_target_name = struct_name.clone().unwrap_or_default();

                        self.emit_array_wrapper_memory_start(
                            ArrayElementType::StructOf(type_id),
                            elements.len(),
                        );
                        for (index, (elem, elem_type)) in
                            elements.iter().zip(elem_types.iter()).enumerate()
                        {
                            self.emit(Instr::PushI64((index + 1) as i64));
                            if is_rational
                                && matches!(
                                    elem_type,
                                    ValueType::I64
                                        | ValueType::I8
                                        | ValueType::I16
                                        | ValueType::I32
                                        | ValueType::I128
                                        | ValueType::U8
                                        | ValueType::U16
                                        | ValueType::U32
                                        | ValueType::U64
                                        | ValueType::U128
                                )
                            {
                                // Promote integer to Rational{Int64}(n, 1)
                                let span = elem.span();
                                let one = Expr::Literal(Literal::Int(1), span);
                                let rational_call = Expr::Call {
                                    function: "Rational{Int64}".to_string(),
                                    args: vec![elem.clone(), one],
                                    kwargs: Vec::new(),
                                    splat_mask: vec![],
                                    kwargs_splat_mask: vec![],
                                    span,
                                };
                                self.compile_expr(&rational_call)?;
                            } else if is_complex
                                && matches!(
                                    elem_type,
                                    ValueType::I64
                                        | ValueType::I8
                                        | ValueType::I16
                                        | ValueType::I32
                                        | ValueType::I128
                                        | ValueType::U8
                                        | ValueType::U16
                                        | ValueType::U32
                                        | ValueType::U64
                                        | ValueType::U128
                                        | ValueType::F64
                                        | ValueType::F32
                                        | ValueType::F16
                                        | ValueType::Bool
                                )
                            {
                                // Promote numeric to Complex{T}(n, 0)
                                let span = elem.span();
                                let zero = Expr::Literal(Literal::Int(0), span);
                                let complex_call = Expr::Call {
                                    function: complex_target_name.clone(),
                                    args: vec![elem.clone(), zero],
                                    kwargs: Vec::new(),
                                    splat_mask: vec![],
                                    kwargs_splat_mask: vec![],
                                    span,
                                };
                                self.compile_expr(&complex_call)?;
                            } else if is_complex
                                && matches!(elem_type, ValueType::Struct(_))
                                && *elem_type != ValueType::Struct(type_id)
                            {
                                // Promote a different Complex type to target Complex type
                                // e.g., Complex{Bool} -> Complex{Int64}
                                // Use Complex{T}(real(z), imag(z)) since struct constructors require 2 args
                                let span = elem.span();
                                let real_call = Expr::Call {
                                    function: "real".to_string(),
                                    args: vec![elem.clone()],
                                    kwargs: Vec::new(),
                                    splat_mask: vec![],
                                    kwargs_splat_mask: vec![],
                                    span,
                                };
                                let imag_call = Expr::Call {
                                    function: "imag".to_string(),
                                    args: vec![elem.clone()],
                                    kwargs: Vec::new(),
                                    splat_mask: vec![],
                                    kwargs_splat_mask: vec![],
                                    span,
                                };
                                let complex_call = Expr::Call {
                                    function: complex_target_name.clone(),
                                    args: vec![real_call, imag_call],
                                    kwargs: Vec::new(),
                                    splat_mask: vec![],
                                    kwargs_splat_mask: vec![],
                                    span,
                                };
                                self.compile_expr(&complex_call)?;
                            } else {
                                self.compile_expr(elem)?;
                            }
                            self.emit(Instr::MemorySet);
                        }
                        self.emit_array_wrapper_from_memory_on_stack(shape);
                        Ok(ValueType::ArrayOf(
                            ArrayElementType::StructOf(type_id),
                            None,
                        ))
                    }
                    ArrayElementType::Bool => {
                        // All boolean elements: use Memory{Bool}.
                        self.emit_array_wrapper_memory_start(
                            ArrayElementType::Bool,
                            elements.len(),
                        );
                        for (index, elem) in elements.iter().enumerate() {
                            self.emit(Instr::PushI64((index + 1) as i64));
                            self.compile_expr_as(elem, ValueType::Bool)?;
                            self.emit(Instr::MemorySet);
                        }
                        self.emit_array_wrapper_from_memory_on_stack(shape);
                        Ok(ValueType::ArrayOf(ArrayElementType::Bool, None))
                    }
                    ArrayElementType::String => {
                        self.emit_array_wrapper_memory_start(
                            ArrayElementType::String,
                            elements.len(),
                        );
                        for (index, elem) in elements.iter().enumerate() {
                            self.emit(Instr::PushI64((index + 1) as i64));
                            self.compile_expr(elem)?;
                            self.emit(Instr::MemorySet);
                        }
                        self.emit_array_wrapper_from_memory_on_stack(shape);
                        Ok(ValueType::ArrayOf(ArrayElementType::String, None))
                    }
                    ArrayElementType::Char => {
                        self.emit_array_wrapper_memory_start(
                            ArrayElementType::Char,
                            elements.len(),
                        );
                        for (index, elem) in elements.iter().enumerate() {
                            self.emit(Instr::PushI64((index + 1) as i64));
                            self.compile_expr(elem)?;
                            self.emit(Instr::MemorySet);
                        }
                        self.emit_array_wrapper_from_memory_on_stack(shape);
                        Ok(ValueType::ArrayOf(ArrayElementType::Char, None))
                    }
                    ArrayElementType::Symbol => {
                        self.emit_array_wrapper_memory_start(
                            ArrayElementType::Symbol,
                            elements.len(),
                        );
                        for (index, elem) in elements.iter().enumerate() {
                            self.emit(Instr::PushI64((index + 1) as i64));
                            self.compile_expr(elem)?;
                            self.emit(Instr::MemorySet);
                        }
                        self.emit_array_wrapper_from_memory_on_stack(shape);
                        Ok(ValueType::ArrayOf(ArrayElementType::Symbol, None))
                    }
                    other => {
                        // Heterogeneous array. Issue #3549: when the inferred
                        // element type is `UnionOf(...)`, propagate it to the
                        // VM so `typeof(a)` reports `Vector{Union{...}}` rather
                        // than `Vector{Any}`. Otherwise fall back to Any.
                        let storage_elem = match &other {
                            ArrayElementType::UnionOf(_)
                            | ArrayElementType::Abstract(_)
                            | ArrayElementType::TupleOf(_) => other.clone(),
                            _ => ArrayElementType::Any,
                        };
                        self.emit_array_wrapper_memory_start(storage_elem, elements.len());
                        for (index, elem) in elements.iter().enumerate() {
                            self.emit(Instr::PushI64((index + 1) as i64));
                            self.compile_expr(elem)?;
                            self.emit(Instr::MemorySet);
                        }
                        self.emit_array_wrapper_from_memory_on_stack(shape);
                        Ok(ValueType::ArrayOf(other, None))
                    }
                }
            }
            Expr::TypedEmptyArray { element_type, span } => {
                // Create empty typed array based on element type string
                // Issue #3548: thread the declared element type all the way through
                // so typeof(Int32[]) reports Vector{Int32}, not Vector{Int64}.
                let elem_type = match element_type.as_str() {
                    "Int" if crate::types::native_int_type_name() == "Int32" => {
                        ArrayElementType::I32
                    }
                    "Int" | "Int64" => ArrayElementType::I64,
                    "Int32" => ArrayElementType::I32,
                    "Int16" => ArrayElementType::I16,
                    "Int8" => ArrayElementType::I8,
                    // Issue #3557: Int128/UInt128 use boxed Any storage with
                    // an element-type override so `typeof(Int128[]) ===
                    // Vector{Int128}`.
                    "Int128" => ArrayElementType::I128,
                    "UInt128" => ArrayElementType::U128,
                    "UInt" if crate::types::native_uint_type_name() == "UInt32" => {
                        ArrayElementType::U32
                    }
                    "UInt" | "UInt64" => ArrayElementType::U64,
                    "UInt32" => ArrayElementType::U32,
                    "UInt16" => ArrayElementType::U16,
                    "UInt8" => ArrayElementType::U8,
                    "Float64" => ArrayElementType::F64,
                    "Float32" => ArrayElementType::F32,
                    "Number" => ArrayElementType::Abstract("Number".to_string()),
                    "Real" => ArrayElementType::Abstract("Real".to_string()),
                    "Integer" => ArrayElementType::Abstract("Integer".to_string()),
                    "Signed" => ArrayElementType::Abstract("Signed".to_string()),
                    "Unsigned" => ArrayElementType::Abstract("Unsigned".to_string()),
                    "AbstractFloat" => ArrayElementType::Abstract("AbstractFloat".to_string()),
                    "Complex{Float64}" | "ComplexF64" => ArrayElementType::ComplexF64,
                    "Complex{Float32}" | "ComplexF32" => ArrayElementType::ComplexF32,
                    "Union{}" | "Bottom" => ArrayElementType::UnionOf(Vec::new()),
                    "Bool" => ArrayElementType::Bool,
                    "String" => ArrayElementType::String,
                    "Char" => ArrayElementType::Char,
                    // Issue #5711: an empty `Symbol[]` / `Regex[]` literal must keep its
                    // declared element type so `eltype` / `typeof` match upstream (the
                    // catch-all below would otherwise widen them to `Any`). `Symbol` has
                    // a dedicated storage tag; `Regex` / `RegexMatch` are boxed scalar
                    // values stored in an `Abstract`-tagged slot (mirrors the non-empty
                    // `Regex[...]` literal, Issue #5706).
                    "Symbol" => ArrayElementType::Symbol,
                    "Regex" => ArrayElementType::Abstract("Regex".to_string()),
                    "RegexMatch" => ArrayElementType::Abstract("RegexMatch".to_string()),
                    "Any" => ArrayElementType::Any,
                    type_name => {
                        // Check if it's a struct type (Complex{Float64}, Point{Int}, etc.)
                        // Extract base name before {
                        let base_name = type_name.split('{').next().unwrap_or(type_name);

                        // Look up struct type in the shared context
                        if let Some(type_id) = self.shared_ctx.get_struct_type_id(base_name) {
                            ArrayElementType::StructOf(type_id)
                        } else if self.locals.contains_key(type_name)
                            || self.shared_ctx.global_types.contains_key(type_name)
                            || self.shared_ctx.global_const_structs.contains_key(type_name)
                            || self.captured_vars.contains(type_name)
                        {
                            // Issue #6839: `name[]` where `name` is a VALUE binding
                            // (a `const` global, local, captured var, or a variable
                            // bound to a type — e.g. `const LOG = Ref(0); LOG[]`, or
                            // `T = Int; T[]`) is `getindex(name)`, NOT the typed
                            // empty-array literal `T[]`. Only genuine type *names*
                            // build an empty `Vector{T}`; recognized builtin types and
                            // user structs are claimed by the arms above and the
                            // `get_struct_type_id` branch, so a value binding only ever
                            // reaches this fallback. Routing to `getindex` lets
                            // dispatch pick the right method — `getindex(::Ref)` reads
                            // the ref, `getindex(::Type{T})` builds the empty vector.
                            let var = Expr::Var(type_name.to_string(), *span);
                            return self.compile_call("getindex", &[var], &[], &[], &[]);
                        } else {
                            // Preserve a concrete parametric eltype
                            // (`UnitRange{Int64}[]`, `Vector{Int}[]`, ...)
                            // rather than widening to `Any` (Issue #6768).
                            concrete_parametric_element_type_from_name(type_name)
                                .unwrap_or(ArrayElementType::Any)
                        }
                    }
                };

                // Emit an empty Memory-backed Array wrapper (Issue #6649).
                self.emit_empty_array_wrapper(elem_type.clone(), &[0]);

                Ok(ValueType::ArrayOf(elem_type, None))
            }
            Expr::Index {
                array,
                indices,
                span,
            } => {
                // `d[k1, k2, ...]` on an AbstractDict is sugar for `d[(k1, k2, ...)]`:
                // upstream defines `getindex(t::AbstractDict, k1, k2, ks...) =
                // getindex(t, tuple(k1, k2, ks...))` (abstractdict.jl). Without this,
                // a Dict receiver with 2+ plain indices falls through to native
                // multi-dim array indexing (`IndexLoad(N)`), which errors on a Dict
                // (Issue #6707). Rewrite to a single tuple key and dispatch the
                // ordinary one-key `getindex`. Slice indices are left alone (a Dict
                // has no slice indexing; let the normal path report the error).
                if indices.len() >= 2
                    && !indices
                        .iter()
                        .any(|idx| matches!(idx, Expr::Range { .. } | Expr::SliceAll { .. }))
                {
                    let receiver_julia = self.infer_julia_type(array);
                    let receiver_is_dict_like = matches!(
                        self.infer_expr_type(array),
                        ValueType::Dict
                    ) || matches!(receiver_julia, JuliaType::Dict)
                        || matches!(&receiver_julia, JuliaType::Struct(name) if is_dict_struct_name(name))
                        || matches!(self.infer_expr_type(array), ValueType::Struct(type_id)
                            if self
                                .shared_ctx
                                .type_id_to_struct_name
                                .get(&type_id)
                                .is_some_and(|name| is_dict_struct_name(name)));
                    if receiver_is_dict_like {
                        let key = Expr::TupleLiteral {
                            elements: indices.clone(),
                            span: *span,
                        };
                        let new_args = vec![array.as_ref().clone(), key];
                        return self.compile_call("getindex", &new_args, &[], &[], &[]);
                    }
                }

                // Julia-compliant: s[i] is equivalent to getindex(s, i)
                // Build arguments for getindex call: [collection, indices...]
                let mut getindex_args = vec![array.as_ref().clone()];
                getindex_args.extend(indices.clone());
                let getindex_arg_types: Vec<JuliaType> = getindex_args
                    .iter()
                    .map(|arg| self.infer_julia_type(arg))
                    .collect();
                // Opaque `ValueType::Dict` receivers must dispatch as Dicts too
                // (Issue #8397): `Dict(x => v)` can widen when `x` comes from a
                // macro/global package value such as `Symbolics.Num`, and falling
                // through to `IndexLoad` treats that non-integer key as an array
                // index.
                let receiver_is_dict_like = match self.infer_expr_type(array) {
                    ValueType::Dict => true,
                    ValueType::Struct(type_id) => self
                        .shared_ctx
                        .type_id_to_struct_name
                        .get(&type_id)
                        .is_some_and(|name| is_dict_struct_name(name)),
                    _ => {
                        matches!(self.infer_julia_type(array), JuliaType::Dict)
                            || matches!(
                                self.infer_julia_type(array),
                                JuliaType::Struct(ref name) if is_dict_struct_name(name)
                            )
                    }
                };
                if receiver_is_dict_like {
                    return self.compile_call("getindex", &getindex_args, &[], &[], &[]);
                }
                let has_slice_like_index = indices.iter().any(|idx| {
                    if matches!(idx, Expr::Range { .. } | Expr::SliceAll { .. }) {
                        return true;
                    }
                    let idx_type = self.infer_expr_type(idx);
                    let idx_julia_type = self.infer_julia_type(idx);
                    is_julia_array_like_type(&idx_julia_type)
                        || self.is_array_wrapper_value_type(&idx_type)
                        || matches!(
                            idx_type,
                            ValueType::Array
                                | ValueType::ArrayOf(_, _)
                                | ValueType::Bool
                                | ValueType::Range
                                | ValueType::Rng
                        )
                });
                if has_slice_like_index
                    && getindex_arg_types
                        .first()
                        .is_some_and(is_julia_array_like_type)
                {
                    return self.compile_builtin_call("getindex", &getindex_args);
                }
                if self.typed_array_literal_element_type(array).is_some() {
                    // `Pair{Int,Int}[...]` and other typed literals must be
                    // materialized by the literal builder before generic
                    // `getindex(::Type, ...)` dispatch can claim the call
                    // (Issue #5233).
                    return self.compile_builtin_call("getindex", &getindex_args);
                }
                if self.has_user_dispatch_method_for_arg_types(
                    &["getindex", "Base.getindex"],
                    &getindex_arg_types,
                ) {
                    return self.compile_call("getindex", &getindex_args, &[], &[], &[]);
                }
                // Issue #6657: an `Any`-typed receiver cannot match a concrete
                // user `getindex` override at compile time, so the check above
                // is false even when the runtime value would dispatch to a user
                // method (e.g. `f(xs) = xs[1]` called with a `Vector` that has a
                // user override). Route it through a runtime dispatch with a
                // native-indexing fallback before the builtin fast path.
                if let Some(result) = self.try_compile_dynamic_getindex_dispatch(&getindex_args) {
                    return result;
                }

                // Special case: typed arrays need IndexLoadTyped for proper type preservation
                let is_typed_array = if let Expr::Var(name, _) = array.as_ref() {
                    matches!(self.locals.get(name), Some(ValueType::ArrayOf(_, _)))
                } else {
                    false
                };

                if is_typed_array {
                    // Check for slice-like indices: Range, SliceAll, Array, or Range variable (Issue #3481)
                    let has_slice = indices.iter().any(|idx| {
                        match idx {
                            Expr::Range { .. } | Expr::SliceAll { .. } => true,
                            _ => {
                                // Array index could be logical indexing (bool array), index array,
                                // or a Range variable
                                let idx_type = self.infer_expr_type(idx);
                                let idx_julia_type = self.infer_julia_type(idx);
                                is_julia_array_like_type(&idx_julia_type)
                                    || self.is_array_wrapper_value_type(&idx_type)
                                    || matches!(
                                        idx_type,
                                        ValueType::Array
                                            | ValueType::ArrayOf(_, _)
                                            | ValueType::Bool
                                            | ValueType::Range
                                            | ValueType::Rng
                                    )
                            }
                        }
                    });

                    // Get return type for typed arrays
                    let has_dynamic_index = indices.iter().any(|idx| {
                        matches!(
                            self.infer_expr_type(idx),
                            ValueType::Any | ValueType::Struct(_)
                        )
                    });
                    let return_type = if has_dynamic_index {
                        None
                    } else if let Expr::Var(name, _) = array.as_ref() {
                        if let Some(ValueType::ArrayOf(elem_type, _)) = self.locals.get(name) {
                            match elem_type {
                                ArrayElementType::StructOf(type_id) => {
                                    Some(ValueType::Struct(*type_id))
                                }
                                ArrayElementType::I64 => Some(ValueType::I64),
                                ArrayElementType::F64 => Some(ValueType::F64),
                                _ => None,
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    self.compile_expr(array)?;
                    for idx in indices {
                        match idx {
                            Expr::Range { .. } | Expr::SliceAll { .. } => {
                                self.compile_expr(idx)?;
                            }
                            _ => {
                                // Check if index might be a CartesianIndex (struct type), Array,
                                // or Range variable (Issue #3481) — compile as-is, no I64 coercion
                                let idx_type = self.infer_expr_type(idx);
                                if matches!(
                                    idx_type,
                                    ValueType::Struct(_)
                                        | ValueType::Any
                                        | ValueType::Array
                                        | ValueType::ArrayOf(_, _)
                                        | ValueType::Bool
                                        | ValueType::Range
                                        | ValueType::Rng
                                ) {
                                    self.compile_expr(idx)?;
                                } else {
                                    self.compile_expr_as(idx, ValueType::I64)?;
                                }
                            }
                        }
                    }
                    if has_slice {
                        self.emit(Instr::IndexSlice(indices.len()));
                        Ok(ValueType::Array)
                    } else if indices.len() == 1
                        && (self.inbounds_context
                            || self.is_proven_inbounds_index(array.as_ref(), &indices[0]))
                    {
                        self.emit(Instr::IndexLoadTypedInbounds(indices.len()));
                        Ok(return_type.unwrap_or(ValueType::Any))
                    } else {
                        self.emit(Instr::IndexLoadTyped(indices.len()));
                        Ok(return_type.unwrap_or(ValueType::Any))
                    }
                } else {
                    // Use getindex builtin for all other types (Dict, Tuple, String, Array)
                    self.compile_builtin_call("getindex", &getindex_args)
                }
            }
            Expr::Range {
                start, step, stop, ..
            } => {
                // Create lazy Range value (does not materialize to array).
                // MakeRangeLazy/MakeStepRangeLazy expect: start, step, stop on stack.
                // An explicit step (`a:s:b`) makes a `StepRange` even if the step is 1
                // (`1:1:5`), distinguished from the `UnitRange` `1:5` (Issue #5667).
                let explicit_step = step.is_some();
                self.compile_expr(start)?;
                if let Some(s) = step {
                    self.compile_expr(s)?;
                } else {
                    self.emit(Instr::PushI64(1));
                }
                self.compile_expr(stop)?;
                self.emit(if explicit_step {
                    Instr::MakeStepRangeLazy
                } else {
                    Instr::MakeRangeLazy
                });
                Ok(ValueType::Range)
            }
            Expr::Comprehension {
                body,
                var,
                iter,
                filter,
                ..
            } => self.compile_comprehension(body, var, iter, filter.as_deref()),
            Expr::MultiComprehension {
                body,
                iterations,
                filter,
                flatten,
                ..
            } => self.compile_multi_comprehension(body, iterations, filter.as_deref(), *flatten),
            Expr::Generator {
                body,
                var,
                iter,
                filter,
                span,
            } => self.compile_generator_expr(body, var, iter, filter.as_deref(), *span),
            Expr::FieldAccess { object, field, .. } => self.compile_field_access(object, field),
            Expr::SliceAll { .. } => {
                self.emit(Instr::SliceAll);
                Ok(ValueType::Array)
            }
            Expr::FunctionRef { name, span } => {
                let _ = span;
                // Check if this function reference is a closure that captures variables
                // from the outer scope (Issue #2358)
                //
                // Lambda functions defined at module level (e.g., in @testset blocks)
                // have their captured variables pre-analyzed during main block setup.
                if let Some(captures) = self.shared_ctx.closure_captures.get(name) {
                    if !captures.is_empty() {
                        // This is a closure - emit CreateClosure instead of PushFunction
                        let capture_names: Vec<String> = captures.iter().cloned().collect();
                        self.emit(Instr::CreateClosure {
                            func_name: name.clone(),
                            capture_names,
                        });
                        return Ok(ValueType::Any);
                    }
                }
                // Regular function reference (not a closure)
                self.emit_function_value(name);
                Ok(ValueType::Function)
            }
            Expr::TupleLiteral { elements, .. } => {
                // Compile each element and create tuple
                for elem in elements {
                    self.compile_expr(elem)?;
                }
                self.emit(Instr::NewTuple(elements.len()));
                Ok(ValueType::Tuple)
            }
            Expr::NamedTupleLiteral { fields, .. } => {
                // Compile each field value and create named tuple
                let names: Vec<String> = fields.iter().map(|(name, _)| name.clone()).collect();
                for (_, value) in fields {
                    self.compile_expr(value)?;
                }
                self.emit(Instr::NewNamedTuple(names));
                Ok(ValueType::NamedTuple)
            }
            Expr::Pair { key, value, .. } => {
                // Issue #4346: `a => b` is a Pair, not a Tuple. Emitting a
                // Tuple lets Pair-specific methods receive the wrong runtime
                // representation after dispatch.
                if let Some(struct_info) = self.shared_ctx.struct_table.get("Pair").cloned() {
                    let args = vec![key.as_ref().clone(), value.as_ref().clone()];
                    self.compile_struct_constructor(struct_info, &args)
                } else {
                    self.compile_expr(key)?;
                    self.compile_expr(value)?;
                    self.emit(Instr::NewTuple(2));
                    Ok(ValueType::Tuple)
                }
            }
            Expr::DictLiteral { pairs, span } => {
                let args: Vec<Expr> = pairs
                    .iter()
                    .map(|(key, value)| Expr::Pair {
                        key: Box::new(key.clone()),
                        value: Box::new(value.clone()),
                        span: *span,
                    })
                    .collect();
                self.compile_call("Dict", &args, &[], &[], &[])
            }
            Expr::LetBlock {
                bindings,
                body,
                span,
            } => {
                // Let blocks introduce local bindings and evaluate the body
                // Track which bindings shadow existing variables so we can restore them
                //
                // FIX for Issue #1361: Store old values in temporary variables instead of
                // on the stack. Using the stack with Swap operations is unsafe when the
                // body contains nested function calls that modify the stack.
                let let_outer_locals = self.locals.clone();
                let let_outer_initialized_locals = self.initialized_locals.clone();
                let let_outer_julia_type_locals = self.julia_type_locals.clone();
                let let_outer_mixed_type_vars = self.mixed_type_vars.clone();
                let let_outer_declared_globals = self.declared_globals.clone();
                let mut shadowed: Vec<(String, ValueType, String)> = Vec::new();
                let mut introduced: Vec<String> = Vec::new();

                // Save old values of variables that will be shadowed to temporary variables
                for (var, _) in bindings {
                    let old_ty_opt = self.locals.get(var).cloned();
                    if let Some(old_ty) = old_ty_opt {
                        if !self.initialized_locals.contains(var) {
                            continue;
                        }
                        // Generate unique temporary variable name using span info
                        let temp_name = format!("__letblock_shadow_{}_{}", var, span.start);
                        // Load old value and store it to temporary variable
                        self.load_local(var)?;
                        self.emit(Instr::StoreAny(temp_name.clone()));
                        shadowed.push((var.clone(), old_ty, temp_name));
                    } else {
                        introduced.push(var.clone());
                    }
                }

                // Store the bindings in locals
                for (var, value) in bindings {
                    let ty = self.compile_expr(value)?;
                    self.locals.insert(var.clone(), ty.clone());
                    self.store_local(var, ty);
                }

                // Compile all statements in the body. Macro-expanded @testset
                // bodies arrive here as LetBlocks containing _testset_begin! /
                // _testset_end!, and should behave as Julia local scopes.
                let opens_testset_scope = block_opens_testset_scope(body);
                let outer_locals = opens_testset_scope.then(|| self.locals.clone());
                let outer_julia_type_locals =
                    opens_testset_scope.then(|| self.julia_type_locals.clone());
                let outer_mixed_type_vars =
                    opens_testset_scope.then(|| self.mixed_type_vars.clone());
                let outer_declared_globals =
                    opens_testset_scope.then(|| self.declared_globals.clone());
                let outer_local_scope_depth = self.local_scope_depth;
                let mut testset_declared_globals = std::collections::HashSet::new();
                if opens_testset_scope {
                    collect_declared_globals_in_testset_scope(body, &mut testset_declared_globals);
                    self.declared_globals
                        .extend(testset_declared_globals.iter().cloned());
                    self.local_scope_depth += 1;
                }
                let result_ty = {
                    let stmts = &body.stmts;
                    let result = if stmts.is_empty() {
                        // Empty block returns nothing
                        self.emit(Instr::PushNothing);
                        Ok(ValueType::Nothing)
                    } else {
                        self.compile_block_value(body)
                    };
                    self.local_scope_depth = outer_local_scope_depth;
                    result?
                };
                if opens_testset_scope {
                    if let Some(outer) = outer_locals {
                        self.locals = outer;
                    }
                    if let Some(outer) = outer_julia_type_locals {
                        self.julia_type_locals = outer;
                    }
                    if let Some(outer) = outer_mixed_type_vars {
                        self.mixed_type_vars = outer;
                    }
                    if let Some(outer) = outer_declared_globals {
                        self.declared_globals = outer;
                    }
                    for name in testset_declared_globals {
                        self.locals.insert(name.clone(), ValueType::Any);
                        self.julia_type_locals.remove(&name);
                        self.mixed_type_vars.insert(name);
                    }
                }

                for var in introduced {
                    self.locals.remove(&var);
                    self.julia_type_locals.remove(&var);
                    self.mixed_type_vars.remove(&var);
                }

                // Restore shadowed variables from temporary storage
                // The result is on top of stack, no need for Swap operations
                for (var, old_ty, temp_name) in shadowed {
                    // Load old value from temporary variable
                    self.emit(Instr::LoadAny(temp_name));
                    // Store it back to the original variable
                    self.store_local(&var, old_ty.clone());
                    self.locals.insert(var, old_ty);
                }
                // Let-local names introduced anywhere in the body must not leak into
                // subsequent branch compilation. Otherwise a later branch can treat
                // them as runtime-shadowed and emit a load for a binding that never
                // existed on that path (Issue #7570).
                if !opens_testset_scope {
                    self.locals = let_outer_locals;
                    self.initialized_locals = let_outer_initialized_locals;
                    self.julia_type_locals = let_outer_julia_type_locals;
                    self.mixed_type_vars = let_outer_mixed_type_vars;
                    self.declared_globals = let_outer_declared_globals;
                }

                Ok(result_ty)
            }
            Expr::StringConcat { parts, .. } => {
                // Compile each part (they will be pushed on the stack)
                for part in parts {
                    self.compile_expr(part)?;
                }
                // Emit StringConcat instruction to concatenate all parts
                self.emit(Instr::StringConcat(parts.len()));
                Ok(ValueType::Str)
            }
            Expr::ModuleCall {
                module,
                function,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                ..
            } => self.compile_module_call(
                module,
                function,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
            ),
            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => {
                let condition_value = {
                    let current_type_for = |name: &str| self.locals.get(name).cloned();
                    super::narrowing::const_nothing_guard_bool(condition, &current_type_for)
                };
                if let Some(condition_value) = condition_value {
                    return if condition_value {
                        self.compile_expr(then_expr)
                    } else {
                        self.compile_expr(else_expr)
                    };
                }

                // Compile: condition ? then_expr : else_expr
                // Similar to if-else but as an expression. Branch-context
                // lowering avoids materializing `&&` / `||` as stack Bools
                // before the conditional jump.
                let condition_false_jumps = self.compile_condition_false_jumps(condition)?;
                let then_restore = self.apply_then_narrowings(condition);
                let then_type = self.compile_expr(then_expr)?;
                self.restore_then_narrowings(then_restore);
                let j_end = self.here();
                self.emit(Instr::Jump(usize::MAX)); // Placeholder

                let else_start = self.here();
                for patch_pos in condition_false_jumps {
                    self.patch_jump(patch_pos, else_start);
                }
                let else_restore = self.apply_else_narrowings(condition);
                let else_type = self.compile_expr(else_expr)?;
                self.restore_then_narrowings(else_restore);

                let end = self.here();
                self.patch_jump(j_end, end);
                // Return the unified type (prefer Any if types differ)
                if then_type == else_type {
                    Ok(then_type)
                } else {
                    Ok(ValueType::Any)
                }
            }
            Expr::New {
                type_args,
                args,
                is_splat,
                span: _,
            } => {
                // `new(args...)` - create a new instance of the enclosing struct
                // For parametric structs, use dynamic struct creation with type bindings
                if let Some(base_name) = self.current_parametric_struct_name.clone() {
                    // Parametric struct: emit NewParametricStruct which resolves type at runtime
                    if *is_splat {
                        return Err(CompileError::Msg(
                            "new(args...) with splat not yet supported for parametric structs"
                                .to_string(),
                        ));
                    }
                    // Explicit `new{A,B}(...)`: when every spelled-out type
                    // parameter resolves to a concrete value at compile time
                    // (either a literal concrete type or a `where`-clause type
                    // variable that is bound from an argument), materialize them
                    // in source order so the instantiation is named & ordered
                    // correctly (e.g. `Swap{Int64, Float64}` instead of dropping
                    // `Float64`). Otherwise fall back to the runtime
                    // type-binding-driven `NewParametricStruct` so we never crash
                    // on an as-yet-unbound parameter (explicit instantiation such
                    // as `Foo{Float64}(1)` still needs call-site type-arg
                    // plumbing — see Issue #5059). (Issue #5059)
                    if !type_args.is_empty()
                        && type_args.iter().all(|ty| self.type_expr_is_resolvable(ty))
                    {
                        for arg in args {
                            self.compile_expr(arg)?;
                        }
                        for ty in type_args {
                            self.compile_type_expr_as_value(ty)?;
                        }
                        self.emit(Instr::NewDynamicParametricStruct(
                            base_name,
                            args.len(),
                            type_args.len(),
                        ));
                        return Ok(ValueType::Any); // Type determined at runtime
                    }
                    for arg in args {
                        self.compile_expr(arg)?;
                    }
                    self.emit(Instr::NewParametricStruct(base_name, args.len()));
                    return Ok(ValueType::Any); // Type determined at runtime
                }
                if let Some(type_id) = self.current_struct_type_id {
                    if *is_splat {
                        // new(args...) - splat a tuple/array into struct fields
                        if args.len() != 1 {
                            return Err(CompileError::Msg(
                                "new(args...) requires exactly one splat argument".to_string(),
                            ));
                        }
                        self.compile_expr(&args[0])?;
                        self.emit(Instr::NewStructSplat(type_id));
                    } else {
                        for arg in args {
                            self.compile_expr(arg)?;
                        }
                        self.emit(Instr::NewStruct(type_id, args.len()));
                    }
                    Ok(ValueType::Struct(type_id))
                } else {
                    Err(CompileError::Msg(
                        "new() is only valid inside inner constructors".to_string(),
                    ))
                }
            }
            Expr::DynamicTypeConstruct {
                base,
                base_expr,
                type_args,
                splat_mask,
                span: _,
            } => {
                // Construct a parametric type at runtime with dynamically evaluated type arguments.
                // Example: Complex{promote_type(T, S)} where T, S are type parameters
                //
                // 1. Compile each type argument expression (evaluates to DataType values)
                // 2. Emit ConstructParametricType[Splat] instruction to build the type

                if let Some(base_expr) = base_expr {
                    if splat_mask.iter().any(|&b| b) {
                        return err(
                            "dynamic parametric type base with splatted parameters is not supported",
                        );
                    }
                    self.compile_expr(base_expr)?;
                    for arg in type_args {
                        self.compile_expr(arg)?;
                    }
                    self.emit(Instr::ApplyTypeDynamic(type_args.len()));
                    return Ok(ValueType::DataType);
                }

                for arg in type_args {
                    self.compile_expr(arg)?;
                }

                // Issue #5112: when any argument is a `...`-splat (`Tuple{xs...}`),
                // emit the splat-aware instruction carrying the per-argument mask
                // so the VM flattens splatted collections before construction.
                if splat_mask.iter().any(|&b| b) {
                    self.emit(Instr::ConstructParametricTypeSplat(
                        base.clone(),
                        splat_mask.clone(),
                    ));
                } else {
                    self.emit(Instr::ConstructParametricType(
                        base.clone(),
                        type_args.len(),
                    ));
                }
                Ok(ValueType::DataType)
            }
            Expr::QuoteLiteral {
                constructor,
                span: _,
            } => {
                // QuoteLiteral contains an expression that constructs the quoted value.
                // Simply compile the constructor expression which produces the Expr/Symbol.
                self.compile_expr(constructor)
            }
            Expr::AssignExpr {
                var,
                value,
                span: _,
            } => {
                // Assignment as expression: compile the value, assign to variable, leave value on stack
                // This is used for chained assignments like `local result = x = 42`
                // The expression evaluates to the assigned value.
                let value_type = self.compile_expr(value)?;

                // Duplicate the value on stack (one for assignment, one for expression result)
                self.emit(Instr::Dup);

                // Store to variable using the standard store_local method
                self.store_local(var, value_type.clone());

                Ok(value_type)
            }
            Expr::ReturnExpr { value, span: _ } => {
                // Return expression: used in short-circuit context like `cond && return x`
                if let Some(val) = value {
                    let value_type = self.compile_expr(val)?;
                    match value_type {
                        ValueType::I64 => self.emit(Instr::ReturnI64),
                        ValueType::F64 => self.emit(Instr::ReturnF64),
                        ValueType::F32 => self.emit(Instr::ReturnF32),
                        ValueType::F16 => self.emit(Instr::ReturnF16),
                        // Use ReturnAny to consume the pushed Nothing value (Issue #2072)
                        ValueType::Nothing => self.emit(Instr::ReturnAny),
                        ValueType::Array | ValueType::ArrayOf(_, _) => {
                            self.emit(Instr::ReturnArray)
                        }
                        ValueType::Struct(_) => self.emit(Instr::ReturnStruct),
                        ValueType::Tuple => self.emit(Instr::ReturnTuple),
                        ValueType::NamedTuple => self.emit(Instr::ReturnNamedTuple),
                        ValueType::Range => self.emit(Instr::ReturnRange),
                        ValueType::Dict => self.emit(Instr::ReturnDict),
                        ValueType::Rng => self.emit(Instr::ReturnRng),
                        _ => self.emit(Instr::ReturnAny),
                    }
                } else {
                    self.emit(Instr::ReturnNothing);
                }
                // Return expressions never produce a value (control flow exits)
                Ok(ValueType::Nothing)
            }
            Expr::BreakExpr { span: _ } => {
                // Break expression: used in short-circuit context like `cond && break`
                if self.loop_stack.is_empty() {
                    return err("break outside of loop");
                }
                let j_exit = self.here();
                self.emit(Instr::Jump(0xDEAD_BEEF)); // placeholder
                if let Some(loop_ctx) = self.loop_stack.last_mut() {
                    loop_ctx.exit_patches.push(j_exit);
                }
                Ok(ValueType::Nothing)
            }
            Expr::ContinueExpr { span: _ } => {
                // Continue expression: used in short-circuit context like `cond && continue`
                if self.loop_stack.is_empty() {
                    return err("continue outside of loop");
                }
                let j_continue = self.here();
                self.emit(Instr::Jump(0xDEAD_BEEF)); // placeholder
                if let Some(loop_ctx) = self.loop_stack.last_mut() {
                    loop_ctx.continue_patches.push(j_continue);
                }
                Ok(ValueType::Nothing)
            }
        }
    }

    fn compile_block_value(&mut self, block: &Block) -> CResult<ValueType> {
        let stmts = &block.stmts;
        if stmts.is_empty() {
            self.emit(Instr::PushNothing);
            return Ok(ValueType::Nothing);
        }

        for stmt in stmts.iter().take(stmts.len() - 1) {
            self.compile_stmt(stmt)?;
        }

        match &stmts[stmts.len() - 1] {
            Stmt::Expr { expr, .. } => self.compile_expr(expr),
            Stmt::Block(block) => self.compile_block_value(block),
            last => {
                self.compile_stmt(last)?;
                self.emit(Instr::PushNothing);
                Ok(ValueType::Nothing)
            }
        }
    }

    pub(super) fn load_local(&mut self, name: &str) -> CResult<()> {
        // A name declared `global x` reads from the module-level (frame 0)
        // binding. Use an explicit global load so slotization cannot rewrite
        // the read to a stale local/testset slot after `StoreGlobalAny` changes
        // the global value's runtime type (Issue #6269).
        if self.declared_globals.contains(name) {
            self.emit(Instr::LoadGlobalAny(name.to_string()));
            return Ok(());
        }

        // Check if this is a captured variable from a closure's outer scope
        if self.captured_vars.contains(name) {
            self.emit(Instr::LoadCaptured(name.to_string()));
            return Ok(());
        }

        // Resolve module constants to qualified names (both in module body and function context)
        // This matches store_local behavior which stores module constants with qualified names
        let (load_name, is_module_constant) = if !self.locals.contains_key(name) {
            // Variable not in locals - check if this is a module constant
            if let Some(module_path) = &self.current_module_path {
                if let Some(const_names) = self.module_constants.get(module_path) {
                    if const_names.contains(name) {
                        (format!("{}.{}", module_path, name), true)
                    } else {
                        (name.to_string(), false)
                    }
                } else {
                    (name.to_string(), false)
                }
            } else {
                (name.to_string(), false)
            }
        } else {
            (name.to_string(), false)
        };

        // For module constants, use the qualified module-level binding.
        if is_module_constant {
            self.emit(Instr::LoadGlobalAny(load_name));
            return Ok(());
        }

        if let Some(qualified) = self.module_private_type_object_name(name) {
            self.emit(Instr::PushDataType(qualified));
            return Ok(());
        }

        // Abstract numeric parameters (`x::Number`, `x::Real`, `x::Integer`, ...)
        // can receive BigInt/BigFloat at runtime. Loading them through the
        // F64/I64 slot selected by the annotation would reject those values before
        // dynamic numeric dispatch has a chance to run (Issue #2498/#4337).
        if self.abstract_numeric_params.contains(name) {
            self.emit(Instr::LoadAny(load_name));
            return Ok(());
        }

        // Prefer local type, fall back to global type (for top-level const/global variables),
        // then default to Any. This ensures functions can access prelude consts like arrays.
        let ty = self
            .locals
            .get(name)
            .cloned()
            .or_else(|| self.shared_ctx.global_types.get(name).cloned())
            .unwrap_or(ValueType::Any);
        if !self.locals.contains_key(name)
            && self.shared_ctx.global_types.contains_key(name)
            && matches!(ty, ValueType::Array | ValueType::ArrayOf(_, _))
        {
            self.emit(Instr::LoadGlobalAny(load_name));
            return Ok(());
        }
        self.emit(match ty {
            ValueType::I64 => Instr::LoadI64(load_name.clone()),
            ValueType::F64 => Instr::LoadF64(load_name.clone()),
            ValueType::F32 => Instr::LoadF32(load_name.clone()),
            ValueType::F16 => Instr::LoadF16(load_name.clone()),
            ValueType::Bool => Instr::LoadBool(load_name.clone()),
            ValueType::Array | ValueType::ArrayOf(_, _) => Instr::LoadArray(load_name.clone()),
            ValueType::Str => Instr::LoadStr(load_name.clone()),
            ValueType::Nothing => Instr::PushNothing, // Nothing is a singleton
            ValueType::Struct(_) => Instr::LoadStruct(load_name.clone()), // All structs including Complex
            ValueType::Rng => Instr::LoadRng(load_name.clone()),
            ValueType::Range => Instr::LoadRange(load_name.clone()),
            ValueType::Tuple => Instr::LoadTuple(load_name.clone()),
            ValueType::NamedTuple => Instr::LoadNamedTuple(load_name.clone()),
            ValueType::Dict => Instr::LoadDict(load_name.clone()),
            // All other types use LoadAny
            _ => Instr::LoadAny(load_name),
        });
        Ok(())
    }

    pub(super) fn store_local(&mut self, name: &str, ty: ValueType) {
        // A name declared `global x` inside a function writes to the module-level
        // (frame 0) binding and must NOT introduce a local slot, so that later
        // reads fall through to the global and the top-level binding is updated
        // (Issues #5548, #5549). `StoreGlobalAny` always targets frame 0.
        if self.declared_globals.contains(name) {
            self.emit(Instr::StoreGlobalAny(name.to_string()));
            return;
        }

        // In module body context (not function), store constants with qualified names
        // so they can be accessed from module functions
        let (store_name, is_module_constant) =
            if !self.strict_undefined_check && self.local_scope_depth == 0 {
                // Module body context - check if this is a module constant
                if let Some(module_path) = &self.current_module_path {
                    if let Some(const_names) = self.module_constants.get(module_path) {
                        if const_names.contains(name) {
                            (format!("{}.{}", module_path, name), true)
                        } else {
                            (name.to_string(), false)
                        }
                    } else {
                        (name.to_string(), false)
                    }
                } else {
                    (name.to_string(), false)
                }
            } else {
                (name.to_string(), false)
            };

        // Don't insert module constants into locals - they're stored in the global frame
        // with qualified names and will be resolved via module_constants lookup
        if !is_module_constant {
            self.locals.insert(name.to_string(), ty.clone());
            self.initialized_locals.insert(name.to_string());
        }
        match ty {
            ValueType::Nothing => {
                // Nothing is a singleton, just pop it from stack
                self.emit(Instr::Pop);
            }
            _ => {
                // Module constants live in the module-level frame under their
                // qualified name so module functions can resolve them.
                if is_module_constant {
                    self.emit(Instr::StoreGlobalAny(store_name));
                    return;
                }

                let instr = match ty {
                    ValueType::I64 => Instr::StoreI64(store_name.clone()),
                    ValueType::F64 => Instr::StoreF64(store_name.clone()),
                    ValueType::F32 => Instr::StoreF32(store_name.clone()),
                    ValueType::F16 => Instr::StoreF16(store_name.clone()),
                    ValueType::Bool => Instr::StoreBool(store_name.clone()),
                    ValueType::Array | ValueType::ArrayOf(_, _) => {
                        Instr::StoreArray(store_name.clone())
                    }
                    ValueType::Str => Instr::StoreStr(store_name.clone()),
                    ValueType::Struct(_) => Instr::StoreStruct(store_name.clone()), // All structs including Complex
                    ValueType::Rng => Instr::StoreRng(store_name.clone()),
                    ValueType::Range => Instr::StoreRange(store_name.clone()),
                    ValueType::Tuple => Instr::StoreTuple(store_name.clone()),
                    ValueType::NamedTuple => Instr::StoreNamedTuple(store_name.clone()),
                    ValueType::Dict => Instr::StoreDict(store_name.clone()),
                    ValueType::Set => Instr::StoreSet(store_name.clone()),
                    // All other types use StoreAny
                    _ => Instr::StoreAny(store_name),
                };
                self.emit(instr)
            }
        }
    }
}
