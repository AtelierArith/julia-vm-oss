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

mod binary;
mod builtin;
mod builtin_array;
mod builtin_hof;
mod builtin_io;
mod builtin_math;
mod builtin_set;
mod builtin_string;
mod builtin_types;
mod call;
mod coercion;
mod collection;
mod infer;
mod struct_;
mod unary;

pub(crate) use infer::infer_array_element_type;

use crate::ir::core::{Expr, Literal, Stmt};
use crate::vm::{ArrayElementType, ArrayValue, Instr, ValueType};
use half::f16;

use super::types::{err, CResult, CompileError};
use super::{
    get_math_constant_value, is_base_function, is_builtin_type_name, is_euler_name, is_pi_name,
    is_random_function, CoreCompiler,
};

impl CoreCompiler<'_> {
    pub(super) fn compile_expr(&mut self, expr: &Expr) -> CResult<ValueType> {
        match expr {
            Expr::Literal(lit, span) => match lit {
                Literal::Int(v) => {
                    self.emit(Instr::PushI64(*v));
                    Ok(ValueType::I64)
                }
                Literal::Int128(v) => {
                    self.emit(Instr::PushI128(*v));
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
                    self.emit(Instr::PushArrayValue(ArrayValue::from_f64(
                        data.clone(),
                        shape.clone(),
                    )));
                    Ok(ValueType::ArrayOf(ArrayElementType::F64))
                }
                Literal::ArrayI64(data, shape) => {
                    self.emit(Instr::PushArrayValue(ArrayValue::from_i64(
                        data.clone(),
                        shape.clone(),
                    )));
                    Ok(ValueType::ArrayOf(ArrayElementType::I64))
                }
                Literal::ArrayBool(data, shape) => {
                    self.emit(Instr::PushArrayValue(ArrayValue::from_bool(
                        data.clone(),
                        shape.clone(),
                    )));
                    Ok(ValueType::ArrayOf(ArrayElementType::Bool))
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
                    // Module literals (e.g., Base, Core) don't have exports/publics info here
                    self.emit(Instr::PushModule(name.clone(), vec![], vec![]));
                    Ok(ValueType::Module)
                }
                Literal::Undef => {
                    // Undef is used for required keyword arguments (no default value)
                    self.emit(Instr::PushUndef);
                    Ok(ValueType::Any)
                }
                // Metaprogramming literals (for REPL persistence)
                Literal::Symbol(name) => {
                    self.emit(Instr::PushSymbol(name.clone()));
                    Ok(ValueType::Any)
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

                // Handle pi/π, NaN, Inf constants (always available without imports)
                if !self.locals.contains_key(name) {
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
                        return Ok(ValueType::ArrayOf(ArrayElementType::String));
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
                        return Ok(ValueType::ArrayOf(ArrayElementType::String));
                    }
                    // LOAD_PATH: Array of load paths (empty in SubsetJuliaVM)
                    if name == "LOAD_PATH" {
                        self.emit(Instr::NewArrayTyped(ArrayElementType::String, 0));
                        self.emit(Instr::FinalizeArrayTyped(vec![0]));
                        return Ok(ValueType::ArrayOf(ArrayElementType::String));
                    }
                    // ENV: Environment variable dictionary (read-only Dict{String,String})
                    if name == "ENV" {
                        self.emit(Instr::PushEnv);
                        return Ok(ValueType::Dict);
                    }
                }
                // Handle type names - push as DataType values for proper Julia semantics
                // Type names like Int64, Float64 are first-class values of type DataType
                if !self.locals.contains_key(name) {
                    // Check if it's a type alias (const MyInt = Int64)
                    // Resolve the alias to its target type
                    if let Some(target_type) = self.shared_ctx.type_aliases.get(name) {
                        self.emit(Instr::PushDataType(target_type.clone()));
                        return Ok(ValueType::DataType);
                    }
                    // Check if it's a built-in type name
                    if is_builtin_type_name(name) {
                        self.emit(Instr::PushDataType(name.to_string()));
                        return Ok(ValueType::DataType);
                    }
                    // Check if it's an abstract type
                    if self.abstract_type_names.contains(name) {
                        self.emit(Instr::PushDataType(name.clone()));
                        return Ok(ValueType::DataType);
                    }
                    // Check if it's a struct type
                    if self.shared_ctx.struct_table.contains_key(name) {
                        self.emit(Instr::PushDataType(name.clone()));
                        return Ok(ValueType::DataType);
                    }
                    // Check if it's a parametric struct type (e.g., Complex, Diagonal)
                    if self.shared_ctx.parametric_structs.contains_key(name) {
                        self.emit(Instr::PushDataType(name.clone()));
                        return Ok(ValueType::DataType);
                    }
                }
                // Resolve bare function names to function objects when not a local variable
                if !self.locals.contains_key(name) {
                    if self.method_tables.contains_key(name) {
                        if !self.imported_functions.contains(name) {
                            return err(format!(
                                "function '{}' is not imported. Use 'using ModuleName' or 'using ModuleName: {}' to import it, or use 'ModuleName.{}()' for qualified access.",
                                name, name, name
                            ));
                        }
                        self.emit(Instr::PushFunction(name.clone()));
                        return Ok(ValueType::Function);
                    }
                    if is_base_function(name) {
                        self.emit(Instr::PushFunction(name.clone()));
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
                // Check if variable is defined (only in strict mode for function bodies)
                // Check locals, globals, const_structs, and captured variables (for closures)
                let in_locals = self.locals.contains_key(name);
                let in_globals = self.shared_ctx.global_types.contains_key(name);
                let in_const_structs = self.shared_ctx.global_const_structs.contains_key(name);
                let in_captured = self.captured_vars.contains(name);
                if self.strict_undefined_check
                    && !in_locals
                    && !in_globals
                    && !in_const_structs
                    && !in_captured
                {
                    // Check if it's a known function name
                    if !self.method_tables.contains_key(name) {
                        return err(format!("Undefined variable: {}", name));
                    }
                }

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
            Expr::UnaryOp { op, operand, .. } => self.compile_unary_op(op, operand),
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

                // Determine array element type based on element types
                let (array_elem_type, _) = infer_array_element_type(
                    &elem_types,
                    |type_id| self.shared_ctx.get_struct_name(type_id),
                    |name| {
                        self.shared_ctx
                            .struct_table
                            .get(name)
                            .map(|info| info.type_id)
                    },
                );

                match array_elem_type {
                    ArrayElementType::I64 => {
                        // All integer elements: use I64 array
                        self.emit(Instr::NewArrayTyped(ArrayElementType::I64, elements.len()));
                        for elem in elements {
                            self.compile_expr_as(elem, ValueType::I64)?;
                            self.emit(Instr::PushElemTyped);
                        }
                        self.emit(Instr::FinalizeArrayTyped(shape.clone()));
                        Ok(ValueType::ArrayOf(ArrayElementType::I64))
                    }
                    ArrayElementType::F64 => {
                        // Numeric elements (with at least one float): use F64 array
                        self.emit(Instr::NewArrayTyped(ArrayElementType::F64, elements.len()));
                        for elem in elements {
                            self.compile_expr_as(elem, ValueType::F64)?;
                            self.emit(Instr::PushElemTyped);
                        }
                        self.emit(Instr::FinalizeArrayTyped(shape.clone()));
                        Ok(ValueType::ArrayOf(ArrayElementType::F64))
                    }
                    ArrayElementType::StructOf(type_id) => {
                        // Struct array - check if we need type promotion (e.g., Int -> Rational, Int -> Complex)
                        let struct_name = self.shared_ctx.get_struct_name(type_id);
                        let is_rational = struct_name
                            .as_ref()
                            .map(|n| n == "Rational" || n.starts_with("Rational{"))
                            .unwrap_or(false);
                        let is_complex = struct_name
                            .as_ref()
                            .map(|n| n.starts_with("Complex"))
                            .unwrap_or(false);
                        // Get the target Complex type name for constructor calls
                        let complex_target_name = struct_name.clone().unwrap_or_default();

                        self.emit(Instr::NewArrayTyped(
                            ArrayElementType::StructOf(type_id),
                            elements.len(),
                        ));
                        for (elem, elem_type) in elements.iter().zip(elem_types.iter()) {
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
                            self.emit(Instr::PushElemTyped);
                        }
                        self.emit(Instr::FinalizeArrayTyped(shape.clone()));
                        Ok(ValueType::ArrayOf(ArrayElementType::StructOf(type_id)))
                    }
                    ArrayElementType::Bool => {
                        // All boolean elements: use Bool array
                        self.emit(Instr::NewArrayTyped(ArrayElementType::Bool, elements.len()));
                        for elem in elements {
                            self.compile_expr_as(elem, ValueType::Bool)?;
                            self.emit(Instr::PushElemTyped);
                        }
                        self.emit(Instr::FinalizeArrayTyped(shape.clone()));
                        Ok(ValueType::ArrayOf(ArrayElementType::Bool))
                    }
                    _ => {
                        // Heterogeneous array (strings, mixed types): use Any element type
                        self.emit(Instr::NewArrayTyped(ArrayElementType::Any, elements.len()));
                        for elem in elements {
                            self.compile_expr(elem)?;
                            self.emit(Instr::PushElemTyped);
                        }
                        self.emit(Instr::FinalizeArrayTyped(shape.clone()));
                        Ok(ValueType::ArrayOf(array_elem_type))
                    }
                }
            }
            Expr::TypedEmptyArray { element_type, .. } => {
                // Create empty typed array based on element type string
                let elem_type = match element_type.as_str() {
                    "Int" | "Int64" => ArrayElementType::I64,
                    "Int32" => ArrayElementType::I64, // Store as I64 internally
                    "Float64" | "Float32" => ArrayElementType::F64,
                    "Bool" => ArrayElementType::Bool,
                    "String" => ArrayElementType::String,
                    "Char" => ArrayElementType::Char,
                    "Any" => ArrayElementType::Any,
                    type_name => {
                        // Check if it's a struct type (Complex{Float64}, Point{Int}, etc.)
                        // Extract base name before {
                        let base_name = type_name.split('{').next().unwrap_or(type_name);

                        // Look up struct type in the shared context
                        if let Some(type_id) = self.shared_ctx.get_struct_type_id(base_name) {
                            ArrayElementType::StructOf(type_id)
                        } else {
                            // Unknown type - use Any
                            ArrayElementType::Any
                        }
                    }
                };

                // Emit instructions for empty typed array
                self.emit(Instr::NewArrayTyped(elem_type.clone(), 0));
                self.emit(Instr::FinalizeArrayTyped(vec![0])); // Empty 1D array

                Ok(ValueType::ArrayOf(elem_type))
            }
            Expr::Index { array, indices, .. } => {
                // Julia-compliant: s[i] is equivalent to getindex(s, i)
                // Build arguments for getindex call: [collection, indices...]
                let mut getindex_args = vec![array.as_ref().clone()];
                getindex_args.extend(indices.clone());

                // Special case: typed arrays need IndexLoadTyped for proper type preservation
                let is_typed_array = if let Expr::Var(name, _) = array.as_ref() {
                    matches!(self.locals.get(name), Some(ValueType::ArrayOf(_)))
                } else {
                    false
                };

                if is_typed_array {
                    // Check for slice-like indices: Range, SliceAll, or Array (for logical indexing)
                    let has_slice = indices.iter().any(|idx| {
                        match idx {
                            Expr::Range { .. } | Expr::SliceAll { .. } => true,
                            _ => {
                                // Array index could be logical indexing (bool array) or index array
                                let idx_type = self.infer_expr_type(idx);
                                matches!(idx_type, ValueType::Array | ValueType::ArrayOf(_))
                            }
                        }
                    });

                    // Get return type for typed arrays
                    let return_type = if let Expr::Var(name, _) = array.as_ref() {
                        if let Some(ValueType::ArrayOf(elem_type)) = self.locals.get(name) {
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
                                // Check if index might be a CartesianIndex (struct type) or Array
                                // If so, compile as Any to let runtime handle it
                                let idx_type = self.infer_expr_type(idx);
                                if matches!(
                                    idx_type,
                                    ValueType::Struct(_)
                                        | ValueType::Any
                                        | ValueType::Array
                                        | ValueType::ArrayOf(_)
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
                // Create lazy Range value (does not materialize to array)
                // MakeRangeLazy expects: start, step, stop on stack
                self.compile_expr(start)?;
                if let Some(s) = step {
                    self.compile_expr(s)?;
                } else {
                    self.emit(Instr::PushI64(1));
                }
                self.compile_expr(stop)?;
                self.emit(Instr::MakeRangeLazy);
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
                ..
            } => self.compile_multi_comprehension(body, iterations, filter.as_deref()),
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
                self.emit(Instr::PushFunction(name.clone()));
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
                // Compile key and value, return as tuple (key, value)
                self.compile_expr(key)?;
                self.compile_expr(value)?;
                self.emit(Instr::NewTuple(2));
                Ok(ValueType::Tuple)
            }
            Expr::DictLiteral { pairs, .. } => {
                // Create a new dict and add all pairs
                self.emit(Instr::NewDict);
                for (key, value) in pairs {
                    self.compile_expr(key)?;
                    self.compile_expr(value)?;
                    self.emit(Instr::DictSet);
                }
                Ok(ValueType::Dict)
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
                let mut shadowed: Vec<(String, ValueType, String)> = Vec::new();

                // Save old values of variables that will be shadowed to temporary variables
                for (var, _) in bindings {
                    let old_ty_opt = self.locals.get(var).cloned();
                    if let Some(old_ty) = old_ty_opt {
                        // Generate unique temporary variable name using span info
                        let temp_name = format!("__letblock_shadow_{}_{}", var, span.start);
                        // Load old value and store it to temporary variable
                        self.load_local(var)?;
                        self.emit(Instr::StoreAny(temp_name.clone()));
                        shadowed.push((var.clone(), old_ty, temp_name));
                    }
                }

                // Store the bindings in locals
                for (var, value) in bindings {
                    let ty = self.compile_expr(value)?;
                    self.locals.insert(var.clone(), ty.clone());
                    self.store_local(var, ty);
                }

                // Compile all statements in the body
                let stmts = &body.stmts;
                let result_ty = if stmts.is_empty() {
                    // Empty block returns nothing
                    self.emit(Instr::PushNothing);
                    ValueType::Nothing
                } else {
                    // Compile all but the last statement
                    for stmt in stmts.iter().take(stmts.len() - 1) {
                        self.compile_stmt(stmt)?;
                    }
                    // The last statement's value is the block's value
                    let last = &stmts[stmts.len() - 1];
                    match last {
                        Stmt::Expr { expr, .. } => self.compile_expr(expr)?,
                        _ => {
                            self.compile_stmt(last)?;
                            self.emit(Instr::PushNothing);
                            ValueType::Nothing
                        }
                    }
                };

                // Restore shadowed variables from temporary storage
                // The result is on top of stack, no need for Swap operations
                for (var, old_ty, temp_name) in shadowed {
                    // Load old value from temporary variable
                    self.emit(Instr::LoadAny(temp_name));
                    // Store it back to the original variable
                    self.store_local(&var, old_ty.clone());
                    self.locals.insert(var, old_ty);
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
                ..
            } => self.compile_module_call(module, function, args, kwargs),
            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => {
                // Compile: condition ? then_expr : else_expr
                // Similar to if-else but as an expression
                self.compile_expr(condition)?;
                let j_else = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX)); // Placeholder
                let then_type = self.compile_expr(then_expr)?;
                let j_end = self.here();
                self.emit(Instr::Jump(usize::MAX)); // Placeholder

                let else_start = self.here();
                self.patch_jump(j_else, else_start);
                let else_type = self.compile_expr(else_expr)?;

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
                args,
                is_splat,
                span: _,
                ..
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
                type_args,
                span: _,
            } => {
                // Construct a parametric type at runtime with dynamically evaluated type arguments.
                // Example: Complex{promote_type(T, S)} where T, S are type parameters
                //
                // 1. Compile each type argument expression (evaluates to DataType values)
                // 2. Emit ConstructParametricType instruction to build the type

                for arg in type_args {
                    self.compile_expr(arg)?;
                }

                self.emit(Instr::ConstructParametricType(
                    base.clone(),
                    type_args.len(),
                ));
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
                        ValueType::Array | ValueType::ArrayOf(_) => self.emit(Instr::ReturnArray),
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

    pub(super) fn load_local(&mut self, name: &str) -> CResult<()> {
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

        // For module constants, use LoadAny since we don't know their type
        if is_module_constant {
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
        self.emit(match ty {
            ValueType::I64 => Instr::LoadI64(load_name.clone()),
            ValueType::F64 => Instr::LoadF64(load_name.clone()),
            ValueType::F32 => Instr::LoadF32(load_name.clone()),
            ValueType::F16 => Instr::LoadF16(load_name.clone()),
            ValueType::Array | ValueType::ArrayOf(_) => Instr::LoadArray(load_name.clone()),
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
        // In module body context (not function), store constants with qualified names
        // so they can be accessed from module functions
        let (store_name, is_module_constant) = if !self.strict_undefined_check {
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
        }
        match ty {
            ValueType::Nothing => {
                // Nothing is a singleton, just pop it from stack
                self.emit(Instr::Pop);
            }
            _ => {
                // For module constants, always use StoreAny for consistency with LoadAny
                if is_module_constant {
                    self.emit(Instr::StoreAny(store_name));
                    return;
                }

                let instr = match ty {
                    ValueType::I64 => Instr::StoreI64(store_name.clone()),
                    ValueType::F64 => Instr::StoreF64(store_name.clone()),
                    ValueType::F32 => Instr::StoreF32(store_name.clone()),
                    ValueType::F16 => Instr::StoreF16(store_name.clone()),
                    ValueType::Array | ValueType::ArrayOf(_) => {
                        Instr::StoreArray(store_name.clone())
                    }
                    ValueType::Str => Instr::StoreStr(store_name.clone()),
                    ValueType::Struct(_) => Instr::StoreStruct(store_name.clone()), // All structs including Complex
                    ValueType::Rng => Instr::StoreRng(store_name.clone()),
                    ValueType::Range => Instr::StoreRange(store_name.clone()),
                    ValueType::Tuple => Instr::StoreTuple(store_name.clone()),
                    ValueType::NamedTuple => Instr::StoreNamedTuple(store_name.clone()),
                    ValueType::Dict => Instr::StoreDict(store_name.clone()),
                    // All other types use StoreAny
                    _ => Instr::StoreAny(store_name),
                };
                self.emit(instr)
            }
        }
    }
}
