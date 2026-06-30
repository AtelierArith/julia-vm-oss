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
                Ok(Some(ValueType::U8))
            }
            "codeunits" => {
                if args.len() != 1 {
                    return err("codeunits requires exactly 1 argument: codeunits(s)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::CodeUnits, 1));
                Ok(Some(ValueType::ArrayOf(ArrayElementType::U8, None)))
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
            // codepoint/bitstring removed - pure Julia (Issue #6747)
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
            // unescape_string removed - now Pure Julia (base/strings/util.jl,
            // Issue #6724); routed DispatchFirst to the method table.
            // isnumeric removed - now Pure Julia (base/strings/unicode.jl,
            // Issue #6752); routed DispatchFirst to the method table so the
            // pure-Julia `isnumeric(c::Char)` (Nd/Nl/No range table) is selected.
            "_substring_retag" => {
                // _substring_retag(v) — internal helper used by split/rsplit so
                // their results show as `SubString{String}["a", "b"]` rather
                // than `["a", "b"]` (Issue #3574). Only changes the array's
                // `element_type_override` to `SubString`; values stay the same.
                if args.len() != 1 {
                    return err(
                        "_substring_retag requires exactly 1 argument: _substring_retag(v)",
                    );
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::SubStringRetag, 1));
                Ok(Some(ValueType::Array))
            }
            "isvalid" => {
                if args.len() != 2 {
                    return err("isvalid requires exactly 2 arguments: isvalid(s, i)");
                }
                self.compile_expr(&args[0])?;
                self.compile_expr_as(&args[1], ValueType::I64)?;
                self.emit(Instr::CallBuiltin(BuiltinId::IsvalidIndex, 2));
                Ok(Some(ValueType::Bool))
            }
            // parse/tryparse for every type (Int64, Bool, Float64) are now Pure
            // Julia (base/parse.jl, Issue #6748). Fall through to method dispatch;
            // the Float64 methods call the `_tryparse_float64` intrinsic below.
            "parse" | "tryparse" => Ok(None),
            "_tryparse_float64" => {
                // _tryparse_float64(s) - libc strtod; Float64 or nothing
                if args.len() != 1 {
                    return err("_tryparse_float64 requires 1 argument");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::TryparseFloat64, 1));
                Ok(Some(ValueType::Any)) // Union{Float64, Nothing}
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
