//! I/O builtin function compilation.
//!
//! Handles compilation of I/O functions: println, print, error, throw, rethrow, IOBuffer, take!, write.

use crate::builtins::BuiltinId;
use crate::bytecode::{ArrayElementType, Instr, ValueType};
use crate::ir::core::Expr;

use super::super::{err, CResult, CoreCompiler};

fn is_iocontext_type_name(name: &str) -> bool {
    let head = name.split_once('{').map_or(name, |(head, _)| head);
    head == "IOContext" || head.ends_with(".IOContext")
}

/// Whether evaluating this print-family argument can have an observable side
/// effect (Issue #10351). Literals and variable reads cannot; anything else
/// (calls, operators, blocks, …) is conservatively treated as effectful.
fn print_arg_may_have_side_effects(arg: &Expr) -> bool {
    !matches!(arg, Expr::Literal(..) | Expr::Var(..))
}

/// Whether a multi-arg `print`/`println` lowering that interleaves writes with
/// argument evaluation must first evaluate every argument (Issue #10351).
/// Upstream evaluates ALL call arguments before `print` runs, so a LATER
/// argument's own output (e.g. `println("x: ", f())` where `f` prints) must
/// appear before any of this call's writes. Only arguments after the first
/// can be reordered relative to an earlier write, so effectful tails decide.
fn print_args_need_spill(args: &[Expr]) -> bool {
    args.len() > 1 && args[1..].iter().any(print_arg_may_have_side_effects)
}

impl CoreCompiler<'_> {
    /// Evaluate `args` left-to-right into fresh temps (Issue #10351), so every
    /// argument's evaluation — including its own output side effects —
    /// completes before the caller emits any write of this call. Returns each
    /// argument's temp name and statically inferred type; `LoadAny` re-pushes
    /// the identical value, so the type remains valid for the typed print
    /// instructions.
    fn spill_print_args(&mut self, args: &[Expr]) -> CResult<Vec<(String, ValueType)>> {
        let mut temps = Vec::with_capacity(args.len());
        for arg in args {
            let ty = self.compile_expr(arg)?;
            let temp = self.new_temp("print_arg");
            self.emit(Instr::StoreAny(temp.clone()));
            temps.push((temp, ty));
        }
        Ok(temps)
    }

    /// Emit the specialized stdout no-newline print for a value of statically
    /// known type already on the stack (shared by `print` and `println`).
    fn emit_stdout_print_no_newline(&mut self, ty: ValueType) {
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
        }
    }

    /// Compile the stdout (no-IO) `print`/`println` argument writes. When a
    /// later argument may have side effects, ALL arguments are evaluated into
    /// temps before the first write (Issue #10351) — upstream evaluates every
    /// call argument before `print` runs, so e.g. `println("x: ", f())` where
    /// `f` itself prints must emit `f`'s output before `"x: "`. The
    /// effect-free case keeps the direct evaluate-and-print loop.
    fn compile_stdout_print_args(&mut self, args: &[Expr]) -> CResult<()> {
        if print_args_need_spill(args) {
            let temps = self.spill_print_args(args)?;
            for (temp, ty) in temps {
                self.emit(Instr::LoadAny(temp));
                self.emit_stdout_print_no_newline(ty);
            }
        } else {
            for arg in args.iter() {
                let ty = self.compile_expr(arg)?;
                self.emit_stdout_print_no_newline(ty);
            }
        }
        Ok(())
    }

    fn is_iocontext_value_type(&self, ty: &ValueType) -> bool {
        let ValueType::Struct(type_id) = ty else {
            return false;
        };
        self.shared_ctx
            .get_struct_name(*type_id)
            .is_some_and(|name| is_iocontext_type_name(&name))
    }

    /// Compile I/O builtin functions.
    /// Returns `Ok(Some(result))` if handled, `Ok(None)` if not an I/O function.
    pub(in super::super) fn compile_builtin_io(
        &mut self,
        name: &str,
        args: &[Expr],
    ) -> CResult<Option<ValueType>> {
        match name {
            "println" => {
                // Julia's println concatenates arguments without adding spaces and
                // appends a newline. Support `println(io, args...)` (Issue #3573):
                // when the first argument is an IO stream (stdout/stderr/IOBuffer/
                // devnull) we route the entire write — newline included — through
                // the IOPrint builtin so that stderr/IOBuffer destinations are
                // honored instead of dumping the io value itself onto stdout.
                if !args.is_empty() {
                    let first_ty = self.infer_expr_type(&args[0]);
                    let first_is_io_like =
                        first_ty == ValueType::IO || self.is_iocontext_value_type(&first_ty);
                    // Issue #4731: when the first arg's type is `Any` (e.g.
                    // `_show_vector(io, v)` where the `io` parameter is
                    // untyped), `println(io)` previously fell through to the
                    // stdout path and `PrintAnyNoNewline` formatted the IO
                    // value with the Rust `Debug` form `IOBuffer(...)`,
                    // leaking that string to stdout. Route `Any`-typed
                    // singles through IOPrint too — IOPrint dispatches on
                    // the runtime kind: IO writes the newline to the sink,
                    // non-IO falls back to printing both [arg, "\n"] to
                    // stdout (matches upstream `println(x)` for any non-IO).
                    // Issue #4853: `println(io, x)` with a single value whose
                    // first arg is statically known to be IO is lowered as
                    // `print(io, x)` followed by a separate newline write, so
                    // the IOPrint two-arg user-`show` dispatch fires for a
                    // struct `x` with a registered `show(io, ::T)` method.
                    // Bundling the trailing "\n" into the same IOPrint call
                    // (the `else` branch below) makes it a three-arg print,
                    // which skips user-`show` dispatch and field-dumps the
                    // struct. We `Dup` the IO value so the side-effecting `io`
                    // expression is evaluated once: stack becomes [io, io],
                    // IOPrint consumes [io, x] (dispatching show), its result
                    // is popped, and `println(io)` writes the newline to the
                    // remaining IO handle.
                    if (first_is_io_like || first_ty == ValueType::Any) && args.len() == 2 {
                        // Single-value `println(io_or_val, x)`. Split the write so
                        // the two-arg IOPrint user-`show` dispatch can fire for a
                        // struct `x` with a registered `show(io, ::T)` method:
                        //   compile(first); Dup        # [first, first]
                        //   compile(x)                 # [first, first, x]
                        //   IOPrint(2)                 # consumes [first, x]
                        //   Pop                        # discard IOPrint result
                        //   IOPrintlnNewline           # newline to first's sink
                        // The `first` value is Dup'd so the (possibly
                        // side-effecting) IO expression is evaluated exactly once.
                        // `IOPrintlnNewline` discriminates `first` at runtime:
                        //   - IO  -> newline to the resolved sink (IOBuffer/
                        //            stdout/stderr/devnull);
                        //   - non-IO (`println(a, x)` with an `Any`-typed non-IO
                        //     `a`) -> newline to stdout. IOPrint already printed
                        //     `a` + `x` to stdout, so the newline is NOT a re-
                        //     print of the value (Issue #4853 follow-up: a local
                        //     `IOBuffer()` infers as `Any`, so this case must be
                        //     runtime-safe rather than assuming a static IO).
                        self.compile_expr(&args[0])?;
                        self.emit(Instr::Dup);
                        self.compile_expr(&args[1])?;
                        self.emit(Instr::CallBuiltin(BuiltinId::IOPrint, 2));
                        self.emit(Instr::Pop);
                        self.emit(Instr::IOPrintlnNewline);
                        self.emit(Instr::PushNothing);
                        return Ok(Some(ValueType::Nothing));
                    }
                    // Issue #4827: multi-value `println(io, a, x, b)` with a
                    // statically-`IO` first arg shares the same gap as multi-arg
                    // `print(io, …)` — bundling every value into one
                    // `IOPrint(N+1)` skips user-`show` dispatch for a struct value
                    // and field-dumps it. Split the same way as `print` above so
                    // each value goes through a two-arg `IOPrint` (which dispatches
                    // `show(io, ::T)`), then write the trailing newline to the
                    // remaining IO handle via `IOPrintlnNewline`:
                    //   compile(io)            # [io]   (evaluated exactly once)
                    //   for each value arg:
                    //     Dup; compile(arg); IOPrint(2); Pop
                    //   IOPrintlnNewline       # consumes io, writes "\n" to sink
                    //   PushNothing            # println returns nothing
                    // Restricted to a statically-`IO` first arg: the `Any` case
                    // (a local `IOBuffer()` infers as `Any`, or a non-IO `a`) keeps
                    // the runtime-dispatched single `IOPrint(N+1)` below.
                    if first_is_io_like && args.len() > 2 {
                        self.compile_expr(&args[0])?;
                        // Evaluate every value argument before the first
                        // 2-arg IOPrint write when a later one may have side
                        // effects (Issue #10351) — upstream evaluates all
                        // call arguments before println writes anything.
                        if print_args_need_spill(args) {
                            let temps = self.spill_print_args(&args[1..])?;
                            for (temp, _ty) in temps {
                                self.emit(Instr::Dup);
                                self.emit(Instr::LoadAny(temp));
                                self.emit(Instr::CallBuiltin(BuiltinId::IOPrint, 2));
                                self.emit(Instr::Pop);
                            }
                        } else {
                            for arg in args[1..].iter() {
                                self.emit(Instr::Dup);
                                self.compile_expr(arg)?;
                                self.emit(Instr::CallBuiltin(BuiltinId::IOPrint, 2));
                                self.emit(Instr::Pop);
                            }
                        }
                        self.emit(Instr::IOPrintlnNewline);
                        self.emit(Instr::PushNothing);
                        return Ok(Some(ValueType::Nothing));
                    }
                    // Issue #7171/#7172: single-arg `println(x)` where `x`'s type
                    // is unknown (`Any`) — e.g. a package/module function return like
                    // `factor(360)::Primes.Factorization`. Bundling `[x, "\n"]` into one
                    // IOPrint sends the value through the non-IO arm's plain field-dump,
                    // skipping a registered `show(io, ::T)`. Split so IOPrint sees the
                    // lone value (and can dispatch user `show`), while `IOPrintlnNewline`
                    // discriminates the sink at runtime — keeping `println(io::Any)`
                    // (newline to the io's sink) correct too (Issue #4731).
                    if first_ty == ValueType::Any && args.len() == 1 {
                        self.compile_expr(&args[0])?;
                        self.emit(Instr::Dup);
                        self.emit(Instr::CallBuiltin(BuiltinId::IOPrint, 1));
                        self.emit(Instr::Pop);
                        self.emit(Instr::IOPrintlnNewline);
                        self.emit(Instr::PushNothing);
                        return Ok(Some(ValueType::Nothing));
                    }
                    if first_is_io_like || first_ty == ValueType::Any {
                        // Compile all user args, then push a literal "\n" as the
                        // final value. IOPrint will dispatch on the first arg's
                        // runtime kind (Stdout/Stderr/Buffer/Devnull/non-IO) and
                        // write each subsequent value — including the trailing
                        // newline — to the resolved sink.
                        for arg in args.iter() {
                            self.compile_expr(arg)?;
                        }
                        self.emit(Instr::PushStr("\n".to_string()));
                        self.emit(Instr::CallBuiltin(BuiltinId::IOPrint, args.len() + 1));
                        return Ok(Some(ValueType::Any));
                    }
                }

                // No IO argument - use specialized print instructions for
                // stdout; all-args-before-any-write ordering per Issue #10351.
                self.compile_stdout_print_args(args)?;
                self.emit(Instr::PrintNewline);
                // Return nothing (Julia's println returns nothing)
                self.emit(Instr::PushNothing);
                Ok(Some(ValueType::Nothing))
            }
            "print" => {
                // Julia's print concatenates arguments without adding spaces
                // Support print(io, x, ...) where first arg is IO - writes to the IO sink
                //
                // When IO is provided:
                //   - print(io, args...) writes to the IO sink and returns nothing
                // When no IO:
                //   - print(args...) writes to stdout and returns nothing
                if !args.is_empty() {
                    // Check if first arg is IO type (compile-time check)
                    let first_ty = self.infer_expr_type(&args[0]);
                    let first_is_io_like =
                        first_ty == ValueType::IO || self.is_iocontext_value_type(&first_ty);
                    // Issue #4827: multi-arg `print(io, a, x, b)` where the first
                    // arg is statically IO must dispatch each struct value `x` to
                    // a user-defined `show(io, ::T)`, just like the 2-arg
                    // `print(io, x)` path already does (Issue #4761). Bundling
                    // every value into one `IOPrint(N)` call skips the user-`show`
                    // dispatch (which the VM only performs for the exact 2-arg
                    // shape) and field-dumps the struct. Split the write so each
                    // value goes through its own two-arg `IOPrint`:
                    //   compile(io)            # [io]   (evaluated exactly once)
                    //   for each value arg:
                    //     Dup                  # [io, io]
                    //     compile(arg)         # [io, io, arg]
                    //     IOPrint(2)           # consumes [io, arg] -> pushes result
                    //     Pop                  # discard the IOPrint result
                    // The original `io` handle is kept only while the split
                    // writes run; Julia's `print` returns `nothing`, so it is
                    // discarded after the final write.
                    // `io` is compiled once and `Dup`'d per value so the
                    // (possibly side-effecting) IO expression is evaluated exactly
                    // once and each two-arg `IOPrint` gets its own copy.
                    //
                    // Restrict the split to a statically-known `IO` first arg: the
                    // `Any`-typed first-arg case may resolve to stdout or a non-IO
                    // value at runtime (e.g. `print(a, x)`), where a per-value
                    // 2-arg IOPrint would change concatenation semantics; that
                    // case keeps the single `IOPrint(N)`.
                    if first_is_io_like && args.len() > 2 {
                        self.compile_expr(&args[0])?;
                        // All-args-before-any-write ordering (Issue #10351),
                        // same as the println split above.
                        if print_args_need_spill(args) {
                            let temps = self.spill_print_args(&args[1..])?;
                            for (temp, _ty) in temps {
                                self.emit(Instr::Dup);
                                self.emit(Instr::LoadAny(temp));
                                self.emit(Instr::CallBuiltin(BuiltinId::IOPrint, 2));
                                self.emit(Instr::Pop);
                            }
                        } else {
                            for arg in args[1..].iter() {
                                self.emit(Instr::Dup);
                                self.compile_expr(arg)?;
                                self.emit(Instr::CallBuiltin(BuiltinId::IOPrint, 2));
                                self.emit(Instr::Pop);
                            }
                        }
                        self.emit(Instr::Pop);
                        self.emit(Instr::PushNothing);
                        return Ok(Some(ValueType::Nothing));
                    }
                    if first_is_io_like || (first_ty == ValueType::Any && args.len() > 1) {
                        // IO is definitely IO, or first arg type is unknown with multiple args
                        // Use IOPrint builtin - it handles both IO and non-IO first args at runtime
                        // Compile all args
                        for arg in args.iter() {
                            self.compile_expr(arg)?;
                        }
                        self.emit(Instr::CallBuiltin(BuiltinId::IOPrint, args.len()));
                        // IOPrint returns `nothing`, matching Julia's `print`.
                        return Ok(Some(ValueType::Any));
                    }
                }

                // No IO argument - use regular print instructions for
                // efficiency; all-args-before-any-write ordering per
                // Issue #10351.
                self.compile_stdout_print_args(args)?;
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
                // throw(value) - throw ANY value as an exception. Upstream Julia
                // allows throwing any value (not just `Exception` subtypes) and
                // `catch` binds the exact thrown value, preserving its identity
                // (e.g. `throw(Int32)` binds the `Int32` DataType itself, not a
                // stringified wrapper). `ThrowValue` pops the value, stashes it
                // verbatim in `pending_exception_value` for catch-binding, and
                // derives a display message from it for the uncaught-exception /
                // backtrace path. Previously non-`Struct`/`Any`/`Str` values were
                // coerced into `ErrorException(string(value))`, losing the
                // original value's type identity (Issue #11554).
                if args.is_empty() {
                    self.emit(Instr::PushStr("error".to_string()));
                    self.emit(Instr::ThrowError);
                } else {
                    self.compile_expr(&args[0])?;
                    self.emit(Instr::ThrowValue);
                }
                Ok(Some(ValueType::Nothing))
            }
            "rethrow" | "Base.rethrow" => {
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
                // IOBuffer() - empty writable buffer; IOBuffer(s) - readable buffer
                // initialized with the string `s` (Issue #5686).
                if args.is_empty() {
                    self.emit(Instr::CallBuiltin(BuiltinId::IOBufferNew, 0));
                    Ok(Some(ValueType::IO))
                } else if args.len() == 1 {
                    self.compile_expr(&args[0])?; // the string content
                    self.emit(Instr::CallBuiltin(BuiltinId::IOBufferFromString, 1));
                    Ok(Some(ValueType::IO))
                } else {
                    err("IOBuffer accepts at most one argument")
                }
            }
            "Pipe" => {
                if !args.is_empty() {
                    return err("Pipe takes no arguments: Pipe()");
                }
                self.emit(Instr::CallBuiltin(BuiltinId::PipeNew, 0));
                Ok(Some(ValueType::IO))
            }
            "redirect_stdout" => {
                if args.len() != 1 && args.len() != 2 {
                    return err(
                        "redirect_stdout requires redirect_stdout(io) or redirect_stdout(f, io)",
                    );
                }
                for arg in args {
                    self.compile_expr(arg)?;
                }
                self.emit(Instr::CallBuiltin(BuiltinId::RedirectStdout, args.len()));
                Ok(Some(if args.len() == 1 {
                    ValueType::IO
                } else {
                    ValueType::Any
                }))
            }
            "redirect_stderr" => {
                if args.len() != 1 && args.len() != 2 {
                    return err(
                        "redirect_stderr requires redirect_stderr(io) or redirect_stderr(f, io)",
                    );
                }
                for arg in args {
                    self.compile_expr(arg)?;
                }
                self.emit(Instr::CallBuiltin(BuiltinId::RedirectStderr, args.len()));
                Ok(Some(if args.len() == 1 {
                    ValueType::IO
                } else {
                    ValueType::Any
                }))
            }
            "take!" => {
                // take!(io) - extract buffered bytes as Vector{UInt8}
                if args.len() != 1 {
                    return err("take! requires exactly 1 argument: take!(io)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::TakeString, 1));
                Ok(Some(ValueType::ArrayOf(ArrayElementType::U8, Some(1))))
            }
            "takestring!" => {
                // Back-compat sjulia alias: upstream no longer exports takestring!,
                // but old sjulia code expected a String result.
                if args.len() != 1 {
                    return err("takestring! requires exactly 1 argument: takestring!(io)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::TakeString, 1));
                self.emit(Instr::CallBuiltin(BuiltinId::StringFromChars, 1));
                Ok(Some(ValueType::Str))
            }
            "write" => {
                // write(io, x) - write bytes to an IO stream, return byte count
                if args.len() != 2 {
                    return err("write requires exactly 2 arguments: write(io, x)");
                }
                self.compile_expr(&args[0])?; // IO
                self.compile_expr(&args[1])?; // value
                self.emit(Instr::CallBuiltin(BuiltinId::IOWrite, 2));
                Ok(Some(ValueType::I64))
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
                            // read(io, String) on an IOBuffer returns its remaining
                            // content (Issue #5686). `take!` extracts the buffer bytes;
                            // String(...) converts them and the read consumes them.
                            if self.infer_expr_type(&args[0]) == ValueType::IO {
                                self.compile_expr(&args[0])?; // the IO buffer
                                self.emit(Instr::CallBuiltin(BuiltinId::TakeString, 1));
                                self.emit(Instr::CallBuiltin(BuiltinId::StringFromChars, 1));
                                return Ok(Some(ValueType::Str));
                            }
                            self.compile_expr(&args[0])?; // filename
                            self.compile_expr(&args[1])?; // String type (ignored at runtime)
                            self.emit(Instr::CallBuiltin(BuiltinId::ReadFile, 2));
                            return Ok(Some(ValueType::Str));
                        }
                        if type_name == "Char" && self.infer_expr_type(&args[0]) == ValueType::IO {
                            self.compile_expr(&args[0])?;
                            self.emit(Instr::CallBuiltin(BuiltinId::ReadCharIo, 1));
                            return Ok(Some(ValueType::Char));
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
                    Some(1),
                )))
            }
            "eachline" => {
                // eachline(filename) - return an iterable collection of lines.
                // This currently materializes the file like readlines(filename),
                // which covers package initialization patterns such as
                // collect(eachline(path)) and map(f, eachline(path)) (Issue #7593).
                if args.len() != 1 {
                    return err("eachline requires exactly 1 argument: eachline(filename)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Eachline, 1));
                Ok(Some(ValueType::ArrayOf(
                    super::super::ArrayElementType::String,
                    Some(1),
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
                    Some(1),
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
            "seek" => {
                // seek(io, pos) - set the file cursor to an absolute byte position
                if args.len() != 2 {
                    return err("seek requires exactly 2 arguments: seek(io, pos)");
                }
                self.compile_expr(&args[0])?;
                self.compile_expr(&args[1])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Seek, 2));
                Ok(Some(ValueType::IO))
            }
            "position" => {
                // position(io) - get the current file cursor byte position
                if args.len() != 1 {
                    return err("position requires exactly 1 argument: position(io)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Position, 1));
                Ok(Some(ValueType::I64))
            }
            "skip" => {
                // skip(io, n) - move the file cursor relative to its current position
                if args.len() != 2 {
                    return err("skip requires exactly 2 arguments: skip(io, n)");
                }
                self.compile_expr(&args[0])?;
                self.compile_expr(&args[1])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Skip, 2));
                Ok(Some(ValueType::IO))
            }
            "flush" => {
                // flush(io) - flush pending writes
                if args.len() != 1 {
                    return err("flush requires exactly 1 argument: flush(io)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Flush, 1));
                Ok(Some(ValueType::Nothing))
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
            "_display_artifact" => {
                // _display_artifact(x) — display-stack hook (Issue #9262). Under a
                // graphical host, render `x` into the display-artifact sink and
                // return `true`; otherwise `false` so `display` falls back to text.
                if args.len() != 1 {
                    return err(
                        "_display_artifact requires exactly 1 argument: _display_artifact(x)",
                    );
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::EmitDisplayArtifact, 1));
                Ok(Some(ValueType::Bool))
            }
            _ => Ok(None),
        }
    }
}
