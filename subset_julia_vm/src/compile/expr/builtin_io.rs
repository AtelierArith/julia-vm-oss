//! I/O builtin function compilation.
//!
//! Handles compilation of I/O functions: println, print, error, throw, rethrow, IOBuffer, take!, write.

use crate::builtins::BuiltinId;
use crate::ir::core::Expr;
use crate::vm::{Instr, ValueType};

use super::super::{err, CResult, CoreCompiler};

impl CoreCompiler<'_> {
    /// Compile I/O builtin functions.
    /// Returns `Ok(Some(result))` if handled, `Ok(None)` if not an I/O function.
    pub(in super::super) fn compile_builtin_io(
        &mut self,
        name: &str,
        args: &[Expr],
    ) -> CResult<Option<ValueType>> {
        match name {
            "println" => {
                // Julia's println concatenates arguments without adding spaces
                for arg in args.iter() {
                    let ty = self.compile_expr(arg)?;
                    match ty {
                        ValueType::I64 => self.emit(Instr::PrintI64NoNewline),
                        ValueType::F64 => self.emit(Instr::PrintF64NoNewline),
                        ValueType::Str => self.emit(Instr::PrintStrNoNewline),
                        ValueType::Nothing => {
                            // For nothing values, pop the Nothing and print "nothing"
                            self.emit(Instr::Pop);
                            self.emit(Instr::PushStr("nothing".to_string()));
                            self.emit(Instr::PrintStrNoNewline);
                        }
                        // Use PrintAnyNoNewline for all other types
                        _ => self.emit(Instr::PrintAnyNoNewline),
                    };
                }
                self.emit(Instr::PrintNewline);
                // Return nothing (Julia's println returns nothing)
                self.emit(Instr::PushNothing);
                Ok(Some(ValueType::Nothing))
            }
            "print" => {
                // Julia's print concatenates arguments without adding spaces
                // Support print(io, x, ...) where first arg is IO - writes to the IOBuffer
                //
                // When IO is provided:
                //   - print(io, args...) writes to the IOBuffer and returns the updated IO
                //   - This allows chaining: io = print(io, "hello")
                // When no IO:
                //   - print(args...) writes to stdout and returns nothing
                if !args.is_empty() {
                    // Check if first arg is IO type (compile-time check)
                    let first_ty = self.infer_expr_type(&args[0]);
                    if first_ty == ValueType::IO || (first_ty == ValueType::Any && args.len() > 1) {
                        // IO is definitely IO, or first arg type is unknown with multiple args
                        // Use IOPrint builtin - it handles both IO and non-IO first args at runtime
                        // Compile all args
                        for arg in args.iter() {
                            self.compile_expr(arg)?;
                        }
                        self.emit(Instr::CallBuiltin(BuiltinId::IOPrint, args.len()));
                        // IOPrint returns IO for IOBuffer, Nothing for stdout/non-IO
                        return Ok(Some(ValueType::Any));
                    }
                }

                // No IO argument - use regular print instructions for efficiency
                for arg in args.iter() {
                    let ty = self.compile_expr(arg)?;
                    match ty {
                        ValueType::I64 => self.emit(Instr::PrintI64NoNewline),
                        ValueType::F64 => self.emit(Instr::PrintF64NoNewline),
                        ValueType::Str => self.emit(Instr::PrintStrNoNewline),
                        ValueType::Nothing => {
                            // For nothing values, pop the Nothing and print "nothing"
                            self.emit(Instr::Pop);
                            self.emit(Instr::PushStr("nothing".to_string()));
                            self.emit(Instr::PrintStrNoNewline);
                        }
                        // Use PrintAnyNoNewline for all other types
                        _ => self.emit(Instr::PrintAnyNoNewline),
                    };
                }
                // Return nothing (Julia's print returns nothing)
                self.emit(Instr::PushNothing);
                Ok(Some(ValueType::Nothing))
            }
            "error" => {
                // error(msg) - throw an ErrorException
                if args.is_empty() {
                    self.emit(Instr::PushStr("error".to_string()));
                } else {
                    // Compile the first argument as the error message
                    let ty = self.compile_expr(&args[0])?;
                    if ty != ValueType::Str {
                        // Convert to string if needed
                        self.emit(Instr::ToStr);
                    }
                }
                self.emit(Instr::ThrowError);
                // error() never returns, but we need a return type for compilation
                Ok(Some(ValueType::Nothing))
            }
            "throw" => {
                // throw(value) - throw any value as an exception
                // For structs (exception types), preserve the original value
                // For strings, use ThrowError for backward compatibility
                if args.is_empty() {
                    self.emit(Instr::PushStr("error".to_string()));
                    self.emit(Instr::ThrowError);
                } else {
                    let ty = self.compile_expr(&args[0])?;
                    match ty {
                        ValueType::Str => {
                            // String exceptions use ThrowError
                            self.emit(Instr::ThrowError);
                        }
                        ValueType::Struct(_) | ValueType::Any => {
                            // Struct exceptions use ThrowValue to preserve the struct
                            self.emit(Instr::ThrowValue);
                        }
                        _ => {
                            // Other types: convert to string and throw
                            self.emit(Instr::ToStr);
                            self.emit(Instr::ThrowError);
                        }
                    }
                }
                Ok(Some(ValueType::Nothing))
            }
            "rethrow" => {
                // rethrow() - rethrow the current exception from within a catch block
                // rethrow(e) - rethrow with a different exception value
                if args.is_empty() {
                    // rethrow() - rethrow current pending exception
                    self.emit(Instr::RethrowCurrent);
                } else if args.len() == 1 {
                    // rethrow(e) - rethrow with new exception value
                    let ty = self.compile_expr(&args[0])?;
                    match ty {
                        ValueType::Str => {
                            // Convert string to throwable format
                            self.emit(Instr::RethrowOther);
                        }
                        _ => {
                            // Structs and other values
                            self.emit(Instr::RethrowOther);
                        }
                    }
                } else {
                    return err("rethrow takes 0 or 1 argument");
                }
                // rethrow() never returns, but we need a return type
                Ok(Some(ValueType::Nothing))
            }
            "IOBuffer" => {
                // IOBuffer() - create new empty IOBuffer
                if !args.is_empty() {
                    return err("IOBuffer() takes no arguments");
                }
                self.emit(Instr::CallBuiltin(BuiltinId::IOBufferNew, 0));
                Ok(Some(ValueType::IO))
            }
            "take!" | "takestring!" => {
                // take!(io) or takestring!(io) - extract string from IOBuffer
                if args.len() != 1 {
                    return err("take!/takestring! requires exactly 1 argument: take!(io)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::TakeString, 1));
                Ok(Some(ValueType::Str))
            }
            "write" => {
                // write(io, x) - write to IOBuffer, return IO for chaining
                if args.len() != 2 {
                    return err("write requires exactly 2 arguments: write(io, x)");
                }
                self.compile_expr(&args[0])?; // IO
                self.compile_expr(&args[1])?; // value
                self.emit(Instr::CallBuiltin(BuiltinId::IOWrite, 2));
                Ok(Some(ValueType::IO))
            }
            "displaysize" => {
                // displaysize() - return terminal size as (rows, cols)
                if !args.is_empty() {
                    return err("displaysize takes no arguments: displaysize()");
                }
                self.emit(Instr::CallBuiltin(BuiltinId::Displaysize, 0));
                Ok(Some(ValueType::Tuple))
            }
            // Note: dirname, basename, joinpath, splitext, splitdir, isabspath, isdirpath
            // are now Pure Julia (base/path.jl) — Issue #2637
            "normpath" => {
                if args.len() != 1 {
                    return err("normpath requires exactly 1 argument: normpath(path)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Normpath, 1));
                Ok(Some(ValueType::Str))
            }
            "abspath" => {
                if args.len() != 1 {
                    return err("abspath requires exactly 1 argument: abspath(path)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Abspath, 1));
                Ok(Some(ValueType::Str))
            }
            "homedir" => {
                if !args.is_empty() {
                    return err("homedir takes no arguments: homedir()");
                }
                self.emit(Instr::CallBuiltin(BuiltinId::Homedir, 0));
                Ok(Some(ValueType::Str))
            }
            // File I/O Operations
            "read" => {
                // read(filename, String) - read entire file as String
                // This handles the 2-argument form for file reading
                if args.len() == 2 {
                    // Check if second arg is String type
                    if let Expr::Var(type_name, _) = &args[1] {
                        if type_name == "String" {
                            self.compile_expr(&args[0])?; // filename
                            self.compile_expr(&args[1])?; // String type (ignored at runtime)
                            self.emit(Instr::CallBuiltin(BuiltinId::ReadFile, 2));
                            return Ok(Some(ValueType::Str));
                        }
                    }
                }
                // Other read overloads not implemented yet
                Ok(None)
            }
            "readlines" => {
                // readlines(filename) - read all lines as Vector{String}
                if args.len() != 1 {
                    return err("readlines requires exactly 1 argument: readlines(filename)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::ReadLines, 1));
                Ok(Some(ValueType::ArrayOf(
                    super::super::ArrayElementType::String,
                )))
            }
            "readline" => {
                // readline(filename) - read first line from file
                // readline(io) - read line from IO stream
                if args.len() != 1 {
                    return err(
                        "readline requires exactly 1 argument: readline(filename) or readline(io)",
                    );
                }
                // Check if argument is IO type
                let arg_ty = self.infer_expr_type(&args[0]);
                self.compile_expr(&args[0])?;
                if arg_ty == ValueType::IO {
                    self.emit(Instr::CallBuiltin(BuiltinId::ReadlineIo, 1));
                } else {
                    self.emit(Instr::CallBuiltin(BuiltinId::Readline, 1));
                }
                Ok(Some(ValueType::Str))
            }
            "countlines" => {
                // countlines(filename) - count lines in file
                if args.len() != 1 {
                    return err("countlines requires exactly 1 argument: countlines(filename)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Countlines, 1));
                Ok(Some(ValueType::I64))
            }
            "isfile" => {
                if args.len() != 1 {
                    return err("isfile requires exactly 1 argument: isfile(path)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Isfile, 1));
                Ok(Some(ValueType::Bool))
            }
            "isdir" => {
                if args.len() != 1 {
                    return err("isdir requires exactly 1 argument: isdir(path)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Isdir, 1));
                Ok(Some(ValueType::Bool))
            }
            "ispath" => {
                if args.len() != 1 {
                    return err("ispath requires exactly 1 argument: ispath(path)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Ispath, 1));
                Ok(Some(ValueType::Bool))
            }
            "filesize" => {
                if args.len() != 1 {
                    return err("filesize requires exactly 1 argument: filesize(path)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Filesize, 1));
                Ok(Some(ValueType::I64))
            }
            "pwd" => {
                if !args.is_empty() {
                    return err("pwd takes no arguments: pwd()");
                }
                self.emit(Instr::CallBuiltin(BuiltinId::Pwd, 0));
                Ok(Some(ValueType::Str))
            }
            "readdir" => {
                // readdir() - list current directory
                // readdir(path) - list specified directory
                if args.len() > 1 {
                    return err("readdir requires 0 or 1 argument: readdir() or readdir(path)");
                }
                if args.is_empty() {
                    // readdir() - use current directory
                    self.emit(Instr::PushStr(".".to_string()));
                    self.emit(Instr::CallBuiltin(BuiltinId::Readdir, 1));
                } else {
                    self.compile_expr(&args[0])?;
                    self.emit(Instr::CallBuiltin(BuiltinId::Readdir, 1));
                }
                Ok(Some(ValueType::ArrayOf(
                    super::super::ArrayElementType::String,
                )))
            }
            "mkdir" => {
                // mkdir(path) - create directory
                if args.len() != 1 {
                    return err("mkdir requires exactly 1 argument: mkdir(path)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Mkdir, 1));
                Ok(Some(ValueType::Str))
            }
            "mkpath" => {
                // mkpath(path) - create directory and all parents
                if args.len() != 1 {
                    return err("mkpath requires exactly 1 argument: mkpath(path)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Mkpath, 1));
                Ok(Some(ValueType::Str))
            }
            "rm" => {
                // rm(path) - remove file or empty directory
                // rm(path; force=false, recursive=false)
                if args.len() != 1 {
                    return err("rm requires exactly 1 argument: rm(path)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Rm, 1));
                Ok(Some(ValueType::Nothing))
            }
            "tempdir" => {
                // tempdir() - get system temp directory
                if !args.is_empty() {
                    return err("tempdir takes no arguments: tempdir()");
                }
                self.emit(Instr::CallBuiltin(BuiltinId::Tempdir, 0));
                Ok(Some(ValueType::Str))
            }
            "tempname" => {
                // tempname() - generate unique temp filename
                if !args.is_empty() {
                    return err("tempname takes no arguments: tempname()");
                }
                self.emit(Instr::CallBuiltin(BuiltinId::Tempname, 0));
                Ok(Some(ValueType::Str))
            }
            "touch" => {
                // touch(path) - create empty file or update mtime
                if args.len() != 1 {
                    return err("touch requires exactly 1 argument: touch(path)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Touch, 1));
                Ok(Some(ValueType::Str))
            }
            "cd" => {
                // cd() - go to home directory
                // cd(path) - change to specified directory
                if args.len() > 1 {
                    return err("cd requires 0 or 1 argument: cd() or cd(path)");
                }
                if args.is_empty() {
                    // cd() - use home directory
                    self.emit(Instr::CallBuiltin(BuiltinId::Homedir, 0));
                    self.emit(Instr::CallBuiltin(BuiltinId::Cd, 1));
                } else {
                    self.compile_expr(&args[0])?;
                    self.emit(Instr::CallBuiltin(BuiltinId::Cd, 1));
                }
                Ok(Some(ValueType::Str))
            }
            "islink" => {
                // islink(path) - check if path is a symbolic link
                if args.len() != 1 {
                    return err("islink requires exactly 1 argument: islink(path)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Islink, 1));
                Ok(Some(ValueType::Bool))
            }
            "cp" => {
                // cp(src, dst) - copy file
                // cp(src, dst; force=false, follow_symlinks=false)
                if args.len() != 2 {
                    return err("cp requires exactly 2 arguments: cp(src, dst)");
                }
                self.compile_expr(&args[0])?;
                self.compile_expr(&args[1])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Cp, 2));
                Ok(Some(ValueType::Str))
            }
            "mv" => {
                // mv(src, dst) - move/rename file
                // mv(src, dst; force=false)
                if args.len() != 2 {
                    return err("mv requires exactly 2 arguments: mv(src, dst)");
                }
                self.compile_expr(&args[0])?;
                self.compile_expr(&args[1])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Mv, 2));
                Ok(Some(ValueType::Str))
            }
            "mtime" => {
                // mtime(path) - get modification time as Unix timestamp
                if args.len() != 1 {
                    return err("mtime requires exactly 1 argument: mtime(path)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Mtime, 1));
                Ok(Some(ValueType::F64))
            }
            "open" => {
                // open(filename) - open file for reading
                // open(filename, mode) - open file with mode ("r", "w", "a", etc.)
                if args.is_empty() || args.len() > 2 {
                    return err(
                        "open requires 1 or 2 arguments: open(filename) or open(filename, mode)",
                    );
                }
                for arg in args.iter() {
                    self.compile_expr(arg)?;
                }
                self.emit(Instr::CallBuiltin(BuiltinId::Open, args.len()));
                Ok(Some(ValueType::IO))
            }
            "close" => {
                // close(io) - close IO stream
                if args.len() != 1 {
                    return err("close requires exactly 1 argument: close(io)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Close, 1));
                Ok(Some(ValueType::Nothing))
            }
            "eof" => {
                // eof(io) - check if at end of file
                if args.len() != 1 {
                    return err("eof requires exactly 1 argument: eof(io)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Eof, 1));
                Ok(Some(ValueType::Bool))
            }
            "isopen" => {
                // isopen(io) - check if IO stream is open
                if args.len() != 1 {
                    return err("isopen requires exactly 1 argument: isopen(io)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Isopen, 1));
                Ok(Some(ValueType::Bool))
            }
            _ => Ok(None),
        }
    }
}
