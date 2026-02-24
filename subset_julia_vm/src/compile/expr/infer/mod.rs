//! Type inference for expression compilation.
//!
//! Handles inference of:
//! - Expression types (ValueType)
//! - Julia types for method dispatch (JuliaType)
//! - Array element types

mod array;
mod hof;
mod julia_type;

pub(crate) use array::infer_array_element_type;

use crate::ir::core::{BinaryOp, BuiltinOp, Expr, Literal, UnaryOp};
use crate::types::JuliaType;
use crate::vm::{ArrayElementType, ValueType};

use crate::compile::{
    binary_op_to_function_name, is_euler_name, is_math_constant, is_pi_name, CoreCompiler,
};

impl CoreCompiler<'_> {
    pub(in crate::compile) fn infer_expr_type(&mut self, expr: &Expr) -> ValueType {
        let _complex_id = self.get_struct_type_id("Complex").unwrap_or(0);
        match expr {
            Expr::Literal(lit, _) => match lit {
                Literal::Int(_) => ValueType::I64,
                Literal::Int128(_) => ValueType::I128,
                Literal::BigInt(_) => ValueType::BigInt,
                Literal::BigFloat(_) => ValueType::BigFloat,
                Literal::Bool(_) => ValueType::Bool,
                Literal::Float(_) => ValueType::F64,
                Literal::Float32(_) => ValueType::F32,
                Literal::Float16(_) => ValueType::F16,
                Literal::Str(_) => ValueType::Str,
                Literal::Char(_) => ValueType::Char,
                Literal::Nothing => ValueType::Nothing,
                Literal::Missing => ValueType::Missing,
                Literal::Module(_) => ValueType::Module,
                Literal::Array(_, _) => ValueType::ArrayOf(ArrayElementType::F64),
                Literal::ArrayI64(_, _) => ValueType::ArrayOf(ArrayElementType::I64),
                Literal::ArrayBool(_, _) => ValueType::ArrayOf(ArrayElementType::Bool),
                Literal::Struct(type_name, _) => {
                    // Look up struct type_id from struct_table
                    if let Some(struct_info) = self.shared_ctx.struct_table.get(type_name) {
                        ValueType::Struct(struct_info.type_id)
                    } else {
                        ValueType::Any
                    }
                }
                Literal::Undef => ValueType::Any, // Required kwarg marker
                // Metaprogramming literals
                Literal::Symbol(_) => ValueType::Any,
                Literal::Expr { .. } => ValueType::Any,
                Literal::QuoteNode(_) => ValueType::Any,
                Literal::LineNumberNode { .. } => ValueType::Any,
                // Regex literal
                Literal::Regex { .. } => ValueType::Regex,
                // Enum literal
                Literal::Enum { .. } => ValueType::Enum,
            },
            Expr::Var(name, _) => {
                // Check if it's a known constant before falling back to locals
                if !self.locals.contains_key(name) {
                    // Check for pi/π (always available)
                    if is_pi_name(name) {
                        return ValueType::F64;
                    }
                    // Check for ℯ (Unicode Euler constant)
                    if is_euler_name(name) {
                        return ValueType::F64;
                    }
                    // Check for NaN and Inf (Float64)
                    if name == "NaN" || name == "Inf" || name == "NaN64" || name == "Inf64" {
                        return ValueType::F64;
                    }
                    // Check for NaN32 and Inf32 (Float32)
                    if name == "NaN32" || name == "Inf32" {
                        return ValueType::F32;
                    }
                    // Check for ENDIAN_BOM (Int32 value for byte order detection)
                    if name == "ENDIAN_BOM" {
                        return ValueType::I64;
                    }
                    // Check for MathConstants when imported via `using Base.MathConstants`
                    if self.usings.contains("Base.MathConstants") && is_math_constant(name) {
                        return ValueType::F64;
                    }
                }
                // Check locals first, then global_types
                // Default to Any (not I64) to ensure dynamic dispatch for unknown types
                self.locals
                    .get(name)
                    .cloned()
                    .or_else(|| self.shared_ctx.global_types.get(name).cloned())
                    .unwrap_or(ValueType::Any)
            }
            Expr::BinaryOp {
                op, left, right, ..
            } => {
                let lt = self.infer_expr_type(left);
                let rt = self.infer_expr_type(right);

                // Check for user-defined operators if either operand is a Struct
                if matches!(lt, ValueType::Struct(_)) || matches!(rt, ValueType::Struct(_)) {
                    // Infer Julia types for method dispatch
                    let left_julia_ty = self.infer_julia_type(left);
                    let right_julia_ty = self.infer_julia_type(right);
                    let op_name = binary_op_to_function_name(op);
                    if let Some(table) = self.method_tables.get(op_name) {
                        let arg_types = vec![left_julia_ty, right_julia_ty];
                        if let Ok(method) = table.dispatch(&arg_types) {
                            return method.return_type.clone();
                        }
                    }
                    // Method dispatch failed but struct operand involved.
                    // Return Any to enable runtime dispatch (fixes Issue #1055).
                    // Comparison operators still return Bool regardless.
                    return match op {
                        BinaryOp::Lt
                        | BinaryOp::Gt
                        | BinaryOp::Le
                        | BinaryOp::Ge
                        | BinaryOp::Eq
                        | BinaryOp::Ne
                        | BinaryOp::And
                        | BinaryOp::Or => ValueType::Bool,
                        _ => ValueType::Any,
                    };
                }

                // Fallback for primitive types (no struct operands)
                // Check if either operand is Any (e.g., untyped function parameter)
                let has_any = lt == ValueType::Any || rt == ValueType::Any;

                match op {
                    BinaryOp::Lt
                    | BinaryOp::Gt
                    | BinaryOp::Le
                    | BinaryOp::Ge
                    | BinaryOp::Eq
                    | BinaryOp::Ne => ValueType::Bool,
                    BinaryOp::And | BinaryOp::Or => ValueType::Bool,
                    BinaryOp::Div => ValueType::F64,
                    BinaryOp::Pow => {
                        // Power operator: String^Int -> Str (repeat), Int^Int -> Int, otherwise -> Float64 (Julia semantics)
                        if lt == ValueType::Str {
                            // String ^ Int returns String (via repeat function)
                            ValueType::Str
                        } else if lt == ValueType::I64 && rt == ValueType::I64 {
                            ValueType::I64
                        } else if has_any {
                            ValueType::Any
                        } else {
                            ValueType::F64
                        }
                    }
                    _ => {
                        // Issue #2127: String/Char concatenation via * returns String
                        if matches!(op, BinaryOp::Mul)
                            && (lt == ValueType::Str
                                || rt == ValueType::Str
                                || (matches!(lt, ValueType::Str | ValueType::Char)
                                    && matches!(rt, ValueType::Str | ValueType::Char)))
                        {
                            return ValueType::Str;
                        }
                        // Array arithmetic: Array +/- Array returns Array
                        let left_is_array = matches!(lt, ValueType::Array | ValueType::ArrayOf(_));
                        let right_is_array = matches!(rt, ValueType::Array | ValueType::ArrayOf(_));
                        if left_is_array || right_is_array {
                            // For array operations, try to preserve element type
                            match (&lt, &rt) {
                                (ValueType::ArrayOf(elem), _) | (_, ValueType::ArrayOf(elem)) => {
                                    ValueType::ArrayOf(elem.clone())
                                }
                                _ => ValueType::Array,
                            }
                        } else if has_any {
                            // If either operand is Any, the result type is determined at runtime
                            // We return Any to signal that dynamic dispatch should be used
                            ValueType::Any
                        } else if lt == ValueType::F64 || rt == ValueType::F64 {
                            ValueType::F64
                        } else if lt == ValueType::F32 || rt == ValueType::F32 {
                            // Issue #1759: Float32 + Bool should return Float32
                            ValueType::F32
                        } else if lt == ValueType::F16 || rt == ValueType::F16 {
                            // Issue #1850: Float16 + Float16 should return Float16
                            ValueType::F16
                        } else {
                            ValueType::I64
                        }
                    }
                }
            }
            Expr::ArrayLiteral { elements, .. } => {
                // Infer element types to determine array element type
                let elem_types: Vec<ValueType> =
                    elements.iter().map(|e| self.infer_expr_type(e)).collect();
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
                ValueType::ArrayOf(array_elem_type)
            }
            Expr::Range { .. } => ValueType::Range,
            Expr::Comprehension { .. } | Expr::MultiComprehension { .. } => ValueType::Array,
            Expr::Generator { .. } => ValueType::Generator,
            Expr::Index { array, indices, .. } => {
                let is_slice = indices
                    .iter()
                    .any(|idx| matches!(idx, Expr::Range { .. } | Expr::SliceAll { .. }));

                // Check if indexing a String
                let array_type = self.infer_expr_type(array);
                if array_type == ValueType::Str {
                    if is_slice {
                        return ValueType::Str; // String slice returns String
                    } else {
                        return ValueType::Char; // String indexing returns Char
                    }
                }

                // Check array type from locals
                if let Expr::Var(name, _) = array.as_ref() {
                    match self.locals.get(name) {
                        Some(ValueType::Dict) => return ValueType::I64,
                        Some(ValueType::Str) => {
                            if is_slice {
                                return ValueType::Str;
                            }
                            return ValueType::Char;
                        }
                        Some(ValueType::ArrayOf(ref elem_type)) => {
                            if is_slice {
                                return ValueType::ArrayOf(elem_type.clone());
                            }
                            // Return element type for single element access
                            return match elem_type {
                                ArrayElementType::I64 => ValueType::I64,
                                ArrayElementType::F64 => ValueType::F64,
                                ArrayElementType::Bool => ValueType::I64, // Bool stored as I64
                                ArrayElementType::StructOf(type_id) => ValueType::Struct(*type_id),
                                ArrayElementType::TupleOf(_) => ValueType::Tuple, // Tuple array access returns Tuple
                                _ => ValueType::Any,
                            };
                        }
                        Some(ValueType::Array) => {
                            if is_slice {
                                return ValueType::Array;
                            }
                            return ValueType::Any; // Array element type determined at runtime
                        }
                        _ => {}
                    }
                }

                // Default for slicing or unknown array
                if is_slice {
                    ValueType::Array
                } else {
                    ValueType::Any // Tuple/array element type determined at runtime
                }
            }
            Expr::SliceAll { .. } => ValueType::Array,
            Expr::Builtin { name, args, .. } => {
                // Infer return type for builtin operations
                match name {
                    // Complex operations (Conj, Real, Imag, Abs, Abs2) are now Pure Julia
                    BuiltinOp::Zero => {
                        // zero returns same type as input
                        if !args.is_empty() {
                            self.infer_expr_type(&args[0])
                        } else {
                            ValueType::F64
                        }
                    }
                    BuiltinOp::Sqrt => {
                        // sqrt(::Complex) is handled by Pure Julia (base/complex.jl)
                        // For primitives, sqrt returns F64
                        if !args.is_empty() {
                            let arg_ty = self.infer_expr_type(&args[0]);
                            if matches!(arg_ty, ValueType::Struct(_)) {
                                // Struct sqrt returns the same struct type
                                arg_ty
                            } else {
                                ValueType::F64
                            }
                        } else {
                            ValueType::F64
                        }
                    }
                    BuiltinOp::Zeros
                    | BuiltinOp::Ones => ValueType::Array,
                    // Note: Trues, Falses, Fill are now Pure Julia — Issue #2640
                    // Note: Adjoint and Transpose are now Pure Julia
                    BuiltinOp::Lu => ValueType::Tuple,
                    BuiltinOp::Det => ValueType::F64,
                    // Note: Inv, TupleLength removed — dead code (Issue #2643)
                    BuiltinOp::Length
                    | BuiltinOp::HasKey
                    | BuiltinOp::TimeNs
                    | BuiltinOp::DictGet => ValueType::I64,
                    BuiltinOp::Rand | BuiltinOp::Randn => {
                        if args.is_empty() {
                            ValueType::F64
                        } else {
                            ValueType::Array
                        }
                    }
                    BuiltinOp::StableRNG | BuiltinOp::XoshiroRNG => ValueType::Rng,
                    BuiltinOp::TupleFirst | BuiltinOp::TupleLast => ValueType::Any, // Tuple element type is unknown
                    BuiltinOp::DictKeys | BuiltinOp::DictValues | BuiltinOp::DictPairs => {
                        ValueType::Tuple
                    }
                    BuiltinOp::DictDelete
                    | BuiltinOp::DictMerge
                    | BuiltinOp::DictMergeBang
                    | BuiltinOp::DictEmpty => ValueType::Dict,
                    BuiltinOp::DictGetBang => ValueType::Any, // Returns the value
                    BuiltinOp::Ref => ValueType::Any,         // Ref can wrap any type
                    BuiltinOp::TypeOf | BuiltinOp::Supertype => ValueType::DataType,
                    BuiltinOp::Isa
                    | BuiltinOp::Isbits
                    | BuiltinOp::Isbitstype
                    | BuiltinOp::Hasfield
                    // Isconcretetype, Isabstracttype, Isprimitivetype, Isstructtype, Ismutabletype
                    // removed - now Pure Julia (base/reflection.jl)
                    | BuiltinOp::Ismutable => ValueType::Bool,
                    BuiltinOp::Sizeof => ValueType::I64,
                    BuiltinOp::Iterate => ValueType::Any, // Returns Tuple or Nothing
                    BuiltinOp::Collect => ValueType::Array,
                    BuiltinOp::Generator => ValueType::Generator, // lazy iterator
                    BuiltinOp::SymbolNew => ValueType::Symbol, // Symbol("name")
                    BuiltinOp::ExprNew => ValueType::Expr, // Expr(head, args...)
                    BuiltinOp::LineNumberNodeNew => ValueType::LineNumberNode, // LineNumberNode(line, file)
                    BuiltinOp::QuoteNodeNew => ValueType::QuoteNode,           // QuoteNode(value)
                    BuiltinOp::GlobalRefNew => ValueType::GlobalRef, // GlobalRef(mod, name)
                    BuiltinOp::Gensym => ValueType::Symbol,          // gensym() or gensym("base")
                    BuiltinOp::Esc => ValueType::Expr,               // esc(expr)
                    BuiltinOp::Eval => ValueType::Any, // eval(expr) - result type is dynamic
                    BuiltinOp::MacroExpand | BuiltinOp::MacroExpandBang => ValueType::Any, // macroexpand returns any type
                    BuiltinOp::IncludeString | BuiltinOp::EvalFile => ValueType::Any, // dynamic code evaluation
                    // Note: BuiltinOp::Zero is already handled above
                    BuiltinOp::IfElse => {
                        if args.len() >= 3 {
                            let then_ty = self.infer_expr_type(&args[1]);
                            let else_ty = self.infer_expr_type(&args[2]);
                            if then_ty == ValueType::F64 || else_ty == ValueType::F64 {
                                ValueType::F64
                            } else {
                                then_ty
                            }
                        } else {
                            ValueType::I64
                        }
                    }
                    _ => ValueType::F64,
                }
            }
            Expr::UnaryOp { op, operand, .. } => {
                match op {
                    UnaryOp::Not => ValueType::Bool,    // ! always returns Bool
                    _ => self.infer_expr_type(operand), // Neg, Pos preserve operand type
                }
            }
            Expr::Call { function, args, .. } => {
                // Check if this is a broadcast call (function name starts with '.')
                // Broadcast operations return Array
                if function.starts_with('.') {
                    return ValueType::Array;
                }

                // Check if this is a type constructor or known builtin function
                match function.as_str() {
                    // abs, abs2, sign: preserve argument type (BigInt, I64, F64, etc.)
                    // Issue #2383: abs(BigInt) should return BigInt, not F64
                    "abs" | "abs2" | "sign" => {
                        if let Some(arg) = args.first() {
                            let arg_type = self.infer_expr_type(arg);
                            match arg_type {
                                ValueType::BigInt => ValueType::BigInt,
                                ValueType::I128 => ValueType::I128,
                                ValueType::I64 => ValueType::I64,
                                ValueType::F32 => ValueType::F32,
                                ValueType::F16 => ValueType::F16,
                                // Complex abs returns F64 (magnitude)
                                ValueType::Struct(_) => ValueType::F64,
                                // Default to F64 for unknown types
                                _ => ValueType::F64,
                            }
                        } else {
                            ValueType::F64
                        }
                    }
                    // gcd, lcm: preserve BigInt type when arguments are BigInt
                    // Issue #2383: gcd(BigInt, BigInt) should return BigInt, not default to F64
                    "gcd" | "lcm" => {
                        // Check if any argument is BigInt
                        let has_bigint = args.iter().any(|arg| {
                            matches!(self.infer_expr_type(arg), ValueType::BigInt)
                        });
                        if has_bigint {
                            ValueType::BigInt
                        } else {
                            // For I64 arguments, gcd/lcm returns I64
                            ValueType::I64
                        }
                    }
                    // Math functions that return F64 for primitive arguments.
                    // IMPORTANT: This list is a type inference hint, independent of whether the
                    // function is a Rust builtin or Pure Julia. Do NOT remove entries during
                    // builtin migration. See Issue #2634 and docs/vm/BUILTIN_REMOVAL.md Layer 5.
                    // Issue #2425: When the argument is a struct type (e.g., Complex),
                    // these functions may return a struct (e.g., log(Complex) -> Complex).
                    // Check for struct arguments and return Any to avoid incorrect F64 inference.
                    "sqrt" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "sinh" | "cosh"
                    | "tanh" | "asinh" | "acosh" | "atanh" | "exp" | "log" | "log2" | "log10"
                    | "log1p" | "expm1" | "floor" | "ceil" | "round" | "trunc"
                    | "signbit" | "prod" | "max" | "min" | "mean" | "std"
                    // Note: sum is now Pure Julia and preserves element type
                    | "var" => {
                        // Check if any argument is a struct or Any type
                        let has_struct_or_any = args.iter().any(|arg| {
                            matches!(self.infer_expr_type(arg), ValueType::Struct(_) | ValueType::Any)
                        });
                        if has_struct_or_any {
                            ValueType::Any
                        } else {
                            ValueType::F64
                        }
                    }
                    // Functions that return I64
                    "length" | "size" | "ndims" | "count" => ValueType::I64,
                    // Functions that return Bool
                    "isnan" | "isinf" | "isfinite" | "isinteger" | "iseven" | "isodd" => {
                        ValueType::Bool
                    }
                    // Functions that return String
                    "string" | "repr" | "uppercase" | "lowercase" | "strip" | "lstrip"
                    | "rstrip" | "chomp" | "chop" | "join" | "take!" | "takestring!" | "sprint"
                    | "sprintf" | "lowercasefirst" | "uppercasefirst" | "escape_string"
                    | "chopprefix" | "chopsuffix" | "replace" | "lpad" | "rpad" | "repeat"
                    | "bitstring" | "ascii" | "unescape_string" => ValueType::Str,
                    // bytes2hex removed - now Pure Julia (base/strings/util.jl)
                    // reverse: returns same type as input (String for string, Array for array)
                    "reverse" => {
                        if let Some(arg) = args.first() {
                            let arg_type = self.infer_expr_type(arg);
                            match arg_type {
                                ValueType::Str => ValueType::Str,
                                ValueType::Array | ValueType::ArrayOf(_) => arg_type,
                                _ => arg_type, // For other types, return the same type
                            }
                        } else {
                            ValueType::F64 // Default fallback
                        }
                    }
                    // Type constructors
                    "Int8" => ValueType::I8,
                    "Int16" => ValueType::I16,
                    "Int32" => ValueType::I32,
                    "Int64" => ValueType::I64,
                    "Int128" => ValueType::I128,
                    "UInt8" => ValueType::U8,
                    "UInt16" => ValueType::U16,
                    "UInt32" => ValueType::U32,
                    "UInt64" => ValueType::U64,
                    "UInt128" => ValueType::U128,
                    "Float32" => ValueType::F32,
                    "Float64" => ValueType::F64,
                    "BigInt" => ValueType::BigInt,
                    "BigFloat" => ValueType::BigFloat,
                    // big() function - converts to BigInt or BigFloat depending on argument
                    "big" => {
                        if let Some(arg) = args.first() {
                            match self.infer_expr_type(arg) {
                                ValueType::F32 | ValueType::F64 => ValueType::BigFloat,
                                _ => ValueType::BigInt,
                            }
                        } else {
                            ValueType::BigInt // Default to BigInt
                        }
                    }
                    // typemin/typemax: return type matches the type argument
                    "typemin" | "typemax" => {
                        if let Some(arg) = args.first() {
                            let julia_ty = self.infer_julia_type(arg);
                            match julia_ty {
                                JuliaType::TypeOf(inner) => match *inner {
                                    JuliaType::Float64 => ValueType::F64,
                                    JuliaType::Float32 => ValueType::F32,
                                    JuliaType::Float16 => ValueType::F16,
                                    JuliaType::Int64 => ValueType::I64,
                                    JuliaType::Int32 => ValueType::I32,
                                    JuliaType::Int16 => ValueType::I16,
                                    JuliaType::Int8 => ValueType::I8,
                                    JuliaType::Int128 => ValueType::I128,
                                    JuliaType::UInt64 => ValueType::U64,
                                    JuliaType::UInt32 => ValueType::U32,
                                    JuliaType::UInt16 => ValueType::U16,
                                    JuliaType::UInt8 => ValueType::U8,
                                    JuliaType::UInt128 => ValueType::U128,
                                    JuliaType::Bool => ValueType::Bool,
                                    _ => ValueType::Any,
                                },
                                _ => ValueType::Any,
                            }
                        } else {
                            ValueType::Any
                        }
                    }
                    // Functions that return DataType
                    "typeof" => ValueType::DataType,
                    "promote_type" => ValueType::DataType,
                    "promote_rule" => ValueType::DataType,
                    "eltype" => ValueType::DataType,
                    "keytype" => ValueType::DataType,
                    "valtype" => ValueType::DataType,
                    // Handle lowercase "complex" function -> Complex struct
                    "complex" => {
                        // Look for Complex{Float64} first (concrete instantiation)
                        if let Some(info) = self.shared_ctx.struct_table.get("Complex{Float64}") {
                            ValueType::Struct(info.type_id)
                        } else if let Some(info) = self.shared_ctx.struct_table.get("Complex") {
                            ValueType::Struct(info.type_id)
                        } else if self.shared_ctx.parametric_structs.contains_key("Complex") {
                            // Complex is a parametric struct - find any instantiation
                            self.shared_ctx
                                .struct_table
                                .iter()
                                .find(|(name, _)| name.starts_with("Complex{"))
                                .map(|(_, info)| ValueType::Struct(info.type_id))
                                .unwrap_or(ValueType::Any)
                        } else {
                            ValueType::Any
                        }
                    }
                    // Dict()/Dict{K,V}() with empty/pair args returns Value::Dict (builtin), not Struct("Dict")
                    // Even though mutable struct Dict{K,V} exists, the compiler intercepts
                    // builtin patterns and emits NewDict. Type inference must match. (Issue #2748)
                    f if f == "Dict" || f.starts_with("Dict{") => {
                        let is_builtin_pattern = args.is_empty()
                            || args.iter().all(|a| matches!(a, Expr::Pair { .. }))
                            || args.len() == 1
                                && matches!(
                                    &args[0],
                                    Expr::Comprehension { .. } | Expr::Generator { .. }
                                );
                        if is_builtin_pattern {
                            return ValueType::Dict;
                        }
                        // Non-builtin pattern falls through to struct constructor
                        if self.shared_ctx.parametric_structs.contains_key("Dict") {
                            let arg_types: Vec<JuliaType> =
                                args.iter().map(|a| self.infer_julia_type(a)).collect();
                            if let Ok(type_args) =
                                self.shared_ctx.infer_type_args("Dict", &arg_types)
                            {
                                if let Ok(type_id) =
                                    self.shared_ctx.resolve_instantiation("Dict", &type_args)
                                {
                                    return ValueType::Struct(type_id);
                                }
                            }
                        }
                        ValueType::Any
                    }
                    _ => {
                        // Check if this is a struct constructor
                        if self.shared_ctx.struct_table.contains_key(function) {
                            if let Some(info) = self.shared_ctx.struct_table.get(function) {
                                ValueType::Struct(info.type_id)
                            } else {
                                ValueType::Any
                            }
                        } else if self.shared_ctx.parametric_structs.contains_key(function) {
                            // Parametric struct constructor - infer type args from arguments
                            let arg_types: Vec<JuliaType> =
                                args.iter().map(|a| self.infer_julia_type(a)).collect();
                            if let Ok(type_args) =
                                self.shared_ctx.infer_type_args(function, &arg_types)
                            {
                                // Build the concrete type name
                                let type_arg_names: Vec<String> =
                                    type_args.iter().map(|t| t.name().to_string()).collect();
                                let concrete_name = if type_arg_names.is_empty() {
                                    function.to_string()
                                } else {
                                    format!("{}{{{}}}", function, type_arg_names.join(", "))
                                };
                                // Look up the type_id from struct_table
                                if let Some(info) = self.shared_ctx.struct_table.get(&concrete_name)
                                {
                                    return ValueType::Struct(info.type_id);
                                }
                                // Instantiate on demand so type inference can be precise for arrays.
                                if let Ok(type_id) =
                                    self.shared_ctx.resolve_instantiation(function, &type_args)
                                {
                                    return ValueType::Struct(type_id);
                                }
                                // Fall back to finding any instantiation of this parametric struct
                                self.shared_ctx
                                    .struct_table
                                    .iter()
                                    .find(|(name, _)| name.starts_with(&format!("{}{{", function)))
                                    .map(|(_, info)| ValueType::Struct(info.type_id))
                                    .unwrap_or(ValueType::Any)
                            } else {
                                self.get_struct_type_id(function)
                                    .map(ValueType::Struct)
                                    .unwrap_or(ValueType::Any)
                            }
                        } else if function.starts_with("Rational{") || function == "Rational" {
                            // Handle parametric Rational constructor
                            // First try exact match, then fall back to any Rational instantiation
                            if let Some(info) = self.shared_ctx.struct_table.get(function) {
                                ValueType::Struct(info.type_id)
                            } else if let Some(type_id) = self.get_struct_type_id("Rational") {
                                ValueType::Struct(type_id)
                            } else {
                                ValueType::Any
                            }
                        } else if function.contains('{') {
                            // Handle parametric struct constructors like Val{1}(), Val{2}(), Point{Int64}(), etc.
                            // Extract base name before the '{'
                            let base_name = &function[..function.find('{').unwrap()];
                            if self.shared_ctx.parametric_structs.contains_key(base_name) {
                                // This is a parametric struct instantiation - use the full name as type
                                // Look up in struct_table first, or create on demand
                                if let Some(info) = self.shared_ctx.struct_table.get(function) {
                                    ValueType::Struct(info.type_id)
                                } else {
                                    // Try to resolve the instantiation
                                    // Parse type args from the function name
                                    let type_args_str = &function
                                        [function.find('{').unwrap() + 1..function.len() - 1];
                                    let type_args: Vec<JuliaType> = type_args_str
                                        .split(',')
                                        .map(|s| JuliaType::from_name_or_struct(s.trim()))
                                        .collect();
                                    if let Ok(type_id) =
                                        self.shared_ctx.resolve_instantiation(base_name, &type_args)
                                    {
                                        ValueType::Struct(type_id)
                                    } else {
                                        // Fall back to finding any instantiation
                                        self.shared_ctx
                                            .struct_table
                                            .iter()
                                            .find(|(name, _)| {
                                                name.starts_with(&format!("{}{{", base_name))
                                            })
                                            .map(|(_, info)| ValueType::Struct(info.type_id))
                                            .unwrap_or(ValueType::Any)
                                    }
                                }
                            } else {
                                ValueType::Any
                            }
                        } else {
                            // Special handling for HOF (Higher-Order Functions) like map/filter
                            // These need call-site specialization to infer the correct return type
                            if function == "map" && args.len() == 2 {
                                // map(f, arr) - infer return type based on f's return type
                                if let Some(return_type) =
                                    self.infer_map_call_return_type(&args[0], &args[1])
                                {
                                    return return_type;
                                }
                            } else if function == "filter" && args.len() == 2 {
                                // filter(pred, arr) - return type is same element type as input
                                if let Some(return_type) = self.infer_filter_call_return_type(&args[1])
                                {
                                    return return_type;
                                }
                            }

                            // Pure Julia functions - try to infer return type from method table
                            if let Some(table) = self.method_tables.get(function.as_str()) {
                                // Infer argument types for dispatch
                                let arg_types: Vec<JuliaType> =
                                    args.iter().map(|a| self.infer_julia_type(a)).collect();
                                // Try to find matching method and get its return type
                                if let Ok(method) = table.dispatch(&arg_types) {
                                    return method.return_type.clone();
                                }
                            }
                            // Fallback to Any if no method matches
                            ValueType::Any
                        }
                    }
                }
            }
            Expr::ModuleCall {
                module,
                function,
                args,
                ..
            } => {
                // Module-qualified function call: Module.func(args)
                let resolved_module = self
                    .module_aliases
                    .get(module.as_str())
                    .map(|s| s.as_str())
                    .unwrap_or(module.as_str());
                if resolved_module == "Base.LinearAlgebra" {
                    return match function.as_str() {
                        "inv" | "svd" | "qr" | "eigen" | "eigvals" | "cholesky" | "rank"
                        | "cond" | "lu" | "det" | "transpose" => match function.as_str() {
                            "det" | "cond" => ValueType::F64,
                            "rank" => ValueType::I64,
                            "svd" | "qr" | "eigen" | "cholesky" => ValueType::NamedTuple,
                            "lu" => ValueType::Tuple,
                            _ => ValueType::Array,
                        },
                        _ => ValueType::Any,
                    };
                }
                // Look up the method table for this function and infer return type
                if let Some(table) = self.method_tables.get(function.as_str()) {
                    let arg_types: Vec<JuliaType> =
                        args.iter().map(|a| self.infer_julia_type(a)).collect();
                    if let Ok(method) = table.dispatch(&arg_types) {
                        return method.return_type.clone();
                    }
                }
                // Fallback to Any if no method matches
                ValueType::Any
            }
            Expr::TupleLiteral { .. } => ValueType::Tuple,
            Expr::NamedTupleLiteral { .. } => ValueType::Tuple,
            Expr::Pair { .. } => ValueType::Tuple,
            // QuoteLiteral produces either Expr or Symbol depending on the constructor
            Expr::QuoteLiteral { constructor, .. } => {
                // Recursively infer the type from the constructor
                self.infer_expr_type(constructor)
            }
            // FieldAccess - check for Expr fields
            Expr::FieldAccess { object, field, .. } => {
                let obj_ty = self.infer_expr_type(object);
                if obj_ty == ValueType::Expr {
                    match field.as_str() {
                        "head" => ValueType::Symbol,
                        "args" => ValueType::Array,
                        _ => ValueType::Any,
                    }
                } else if let ValueType::Struct(type_id) = obj_ty {
                    // Look up field type from struct definition
                    for (_, struct_info) in self.shared_ctx.struct_table.iter() {
                        if struct_info.type_id == type_id {
                            for (field_name, field_ty) in &struct_info.fields {
                                if field_name == field {
                                    return field_ty.clone();
                                }
                            }
                            break;
                        }
                    }
                    ValueType::Any
                } else {
                    ValueType::Any
                }
            }
            Expr::TypedEmptyArray { element_type, .. } => {
                // Typed empty array like Bool[], Int64[], Float64[]
                match element_type.as_str() {
                    "Int" | "Int64" => ValueType::ArrayOf(ArrayElementType::I64),
                    "Int32" => ValueType::ArrayOf(ArrayElementType::I64), // Store as I64
                    "Float64" | "Float32" => ValueType::ArrayOf(ArrayElementType::F64),
                    "Bool" => ValueType::ArrayOf(ArrayElementType::Bool),
                    "String" => ValueType::ArrayOf(ArrayElementType::String),
                    "Char" => ValueType::ArrayOf(ArrayElementType::Char),
                    "Any" => ValueType::ArrayOf(ArrayElementType::Any),
                    type_name => {
                        // Check if it's a struct type
                        let base_name = type_name.split('{').next().unwrap_or(type_name);
                        if let Some(type_id) = self.shared_ctx.get_struct_type_id(base_name) {
                            ValueType::ArrayOf(ArrayElementType::StructOf(type_id))
                        } else {
                            ValueType::ArrayOf(ArrayElementType::Any)
                        }
                    }
                }
            }
            // Default fallback - use Any instead of F64 to avoid type mismatches
            _ => ValueType::Any,
        }
    }
}
