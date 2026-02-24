//! String builtin function compilation.
//!
//! Handles compilation of string functions: split, join, etc.
//! Note: uppercase, lowercase, titlecase are now Pure Julia (base/strings/unicode.jl)

use crate::builtins::BuiltinId;
use crate::ir::core::Expr;
use crate::vm::value::ArrayElementType;
use crate::vm::{Instr, ValueType};

use super::super::{err, CResult, CoreCompiler};

impl CoreCompiler<'_> {
    /// Compile string builtin functions.
    /// Returns `Ok(Some(result))` if handled, `Ok(None)` if not a string function.
    pub(in super::super) fn compile_builtin_string(
        &mut self,
        name: &str,
        args: &[Expr],
    ) -> CResult<Option<ValueType>> {
        match name {
            "uppercase" | "lowercase" => {
                // Now handled by Pure Julia (base/strings/unicode.jl)
                // Return None to let compile_call try method_tables
                Ok(None)
            }
            "titlecase" => {
                // Now handled by Pure Julia (base/strings/unicode.jl)
                // Return None to let compile_call try method_tables
                Ok(None)
            }
            // Note: strip, lstrip, rstrip, chomp, chop, startswith, endswith, occursin are now Pure Julia functions
            // in subset_julia_vm/src/julia/base/strings.jl
            // findfirst, findlast, findnext, findprev removed - now Pure Julia (base/strings/search.jl)
            "ncodeunits" => {
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Ncodeunits, 1));
                Ok(Some(ValueType::I64))
            }
            "codeunit" => {
                if args.len() != 2 {
                    return err("codeunit requires exactly 2 arguments: codeunit(s, i)");
                }
                self.compile_expr(&args[0])?;
                self.compile_expr(&args[1])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Codeunit, 2));
                Ok(Some(ValueType::I64))
            }
            "codeunits" => {
                if args.len() != 1 {
                    return err("codeunits requires exactly 1 argument: codeunits(s)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::CodeUnits, 1));
                Ok(Some(ValueType::ArrayOf(ArrayElementType::U8)))
            }
            "repeat" => {
                // All repeat calls (String, Array, etc.) are handled by Pure Julia
                // Return None to let compile_call try method_tables
                Ok(None)
            }
            "split" => {
                // String split is now handled by Pure Julia (base/strings/util.jl)
                // Return None to let compile_call try method_tables
                Ok(None)
            }
            "rsplit" => {
                // String rsplit is now handled by Pure Julia (base/strings/util.jl)
                // Return None to let compile_call try method_tables
                Ok(None)
            }
            // Note: join is now Pure Julia (base/strings.jl)
            "string" => {
                // string(args...) - concatenate all arguments into a string
                for arg in args.iter() {
                    self.compile_expr(arg)?;
                }
                self.emit(Instr::CallBuiltin(BuiltinId::StringNew, args.len()));
                Ok(Some(ValueType::Str))
            }
            // Note: repr is now implemented in Pure Julia (base/io.jl)
            // It uses show(io, x) to get the string representation.
            "bitstring" => {
                // bitstring(x) - binary representation as string
                if args.len() != 1 {
                    return err("bitstring requires exactly 1 argument: bitstring(x)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Bitstring, 1));
                Ok(Some(ValueType::Str))
            }
            "codepoint" => {
                // codepoint(c) - Unicode codepoint as UInt32
                if args.len() != 1 {
                    return err("codepoint requires exactly 1 argument: codepoint(c::Char)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Codepoint, 1));
                Ok(Some(ValueType::U32))
            }
            // "ascii" removed - now Pure Julia in base/strings/util.jl
            // nextind, prevind, thisind, reverseind removed - now Pure Julia (base/strings/basic.jl)
            // bytes2hex, hex2bytes removed - now Pure Julia (base/strings/util.jl)
            "sprintf" => {
                // sprintf(fmt, args...) - formatted string
                if args.is_empty() {
                    return err("sprintf requires at least 1 argument: sprintf(fmt, args...)");
                }
                for arg in args.iter() {
                    self.compile_expr(arg)?;
                }
                self.emit(Instr::CallBuiltin(BuiltinId::Sprintf, args.len()));
                Ok(Some(ValueType::Str))
            }
            "unescape_string" => {
                // unescape_string(s) - unescape escape sequences
                if args.is_empty() || args.len() > 2 {
                    return err("unescape_string requires 1 or 2 arguments: unescape_string(s) or unescape_string(s, keep)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::UnescapeString, 1));
                Ok(Some(ValueType::Str))
            }
            "isnumeric" => {
                // isnumeric(c) - check if character is numeric (Unicode)
                if args.len() != 1 {
                    return err("isnumeric requires exactly 1 argument: isnumeric(c)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Isnumeric, 1));
                Ok(Some(ValueType::Bool))
            }
            "isvalid" => {
                // isvalid(s, i) - check if index is valid character boundary
                if args.len() != 2 {
                    return err("isvalid requires exactly 2 arguments: isvalid(s, i)");
                }
                self.compile_expr(&args[0])?;
                self.compile_expr(&args[1])?;
                self.emit(Instr::CallBuiltin(BuiltinId::IsvalidIndex, 2));
                Ok(Some(ValueType::Bool))
            }
            // tryparse(Int64, s) and parse(Int64, s) are now Pure Julia (base/parse.jl)
            "tryparse" => {
                // tryparse(T, s) - only Float64 remains as builtin
                if args.len() != 2 {
                    return Ok(None); // Fall through to method dispatch
                }
                let type_name = match &args[0] {
                    Expr::Var(name, _) => name.as_str(),
                    _ => return Ok(None),
                };
                match type_name {
                    "Float64" | "Float" => {
                        self.compile_expr(&args[1])?;
                        self.emit(Instr::CallBuiltin(BuiltinId::TryparseFloat64, 1));
                        Ok(Some(ValueType::Any)) // Returns Union{Float64, Nothing}
                    }
                    _ => Ok(None), // Int64 etc. handled by Pure Julia
                }
            }
            "parse" => {
                // parse(T, s) - only Float64 remains as builtin
                if args.len() != 2 {
                    return Ok(None); // Fall through to method dispatch
                }
                let type_name = match &args[0] {
                    Expr::Var(name, _) => name.as_str(),
                    _ => return Ok(None),
                };
                match type_name {
                    "Float64" | "Float" => {
                        self.compile_expr(&args[1])?;
                        self.emit(Instr::CallBuiltin(BuiltinId::StringToFloat, 1));
                        Ok(Some(ValueType::F64))
                    }
                    _ => Ok(None), // Int64 etc. handled by Pure Julia
                }
            }
            "_regex_replace" => {
                // _regex_replace(string, regex, replacement, count) - Issue #2112
                if args.len() != 4 {
                    return err(
                        "_regex_replace requires 4 arguments: _regex_replace(s, regex, new, count)",
                    );
                }
                for arg in args {
                    self.compile_expr(arg)?;
                }
                self.emit(Instr::CallBuiltin(BuiltinId::RegexReplace, 4));
                Ok(Some(ValueType::Str))
            }
            _ => Ok(None),
        }
    }
}
