# =============================================================================
# errorshow.jl - Exception display with showerror
# =============================================================================
# Based on Julia's base/errorshow.jl
#
# This module implements the `showerror` function for customizing how
# exceptions are displayed. It provides default implementations for
# all exception types defined in error.jl.
#
# The internal `_showerror_str` helpers keep each exception's formatting in one
# place; public `showerror(io, ex)` writes those strings to the supplied IO.

# =============================================================================
# Internal string-based implementations
# =============================================================================
# These functions return strings and are used by the public showerror API.

# Default: just return the type name
function _showerror_str(ex)
    return string(typeof(ex))
end

# ErrorException
function _showerror_str(ex::ErrorException)
    return ex.msg
end

# DimensionMismatch
function _showerror_str(ex::DimensionMismatch)
    return string("DimensionMismatch: ", ex.msg)
end

# KeyError
function _showerror_str(ex::KeyError)
    return string("KeyError: key ", ex.key, " not found")
end

# StringIndexError
function _showerror_str(ex::StringIndexError)
    return string("StringIndexError: invalid index [", ex.index, "]")
end

# BoundsError
function _showerror_str(ex::BoundsError)
    if ex.a === nothing
        return "BoundsError"
    end
    # Upstream renders a tuple index as its joined components:
    # `at index [9, 9]`, not `[(9, 9)]` (Issue #11374).
    idx = ex.i isa Tuple ? join(ex.i, ", ") : ex.i
    return string("BoundsError: attempt to access ", typeof(ex.a), " at index [", idx, "]")
end

# OverflowError
function _showerror_str(ex::OverflowError)
    return string("OverflowError: ", ex.msg)
end

# StackOverflowError
function _showerror_str(ex::StackOverflowError)
    return "StackOverflowError:"
end

# OutOfMemoryError
function _showerror_str(ex::OutOfMemoryError)
    return "OutOfMemoryError()"
end

# UndefRefError
function _showerror_str(ex::UndefRefError)
    return "UndefRefError: access to undefined reference"
end

# AssertionError
function _showerror_str(ex::AssertionError)
    return string("AssertionError: ", ex.msg)
end

# DivideError
function _showerror_str(ex::DivideError)
    return "DivideError: integer division error"
end

# DomainError
function _showerror_str(ex::DomainError)
    if ex.msg == ""
        return string("DomainError with ", ex.val)
    else
        return string("DomainError with ", ex.val, ":\n", ex.msg)
    end
end

# InexactError
# Mirrors Julia 1.12's `(func, args)` layout. The target type is omitted when
# `nameof(args[1]) === ex.func` (e.g. `InexactError: Int64(1.5)`) (Issue #8732).
function _showerror_str(ex::InexactError)
    args = ex.args
    n = length(args)
    if n == 2 && nameof(args[1]) === ex.func
        return string("InexactError: ", ex.func, "(", args[2], ")")
    end
    result = string("InexactError: ", ex.func, "(")
    i = 1
    while i <= n
        if i > 1
            result = string(result, ", ")
        end
        result = string(result, args[i])
        i = i + 1
    end
    return string(result, ")")
end

# TypeError
# Mirrors julia/base/errorshow.jl `showerror(io::IO, ex::TypeError)` (Issue #5146).
# `ex.got` holds the offending VALUE (not its type); we format it as
# "a value of type $(typeof(ex.got))", or "Type{...}" when the value is itself a
# type. The `expected === Bool` case yields the "non-boolean (...)" message.
function _showerror_str(ex::TypeError)
    if ex.expected === Bool
        return string("TypeError: non-boolean (", typeof(ex.got), ") used in boolean context")
    end
    if isa(ex.got, Type)
        targ = string("Type{", ex.got, "}")
    else
        targ = string("a value of type ", typeof(ex.got))
    end
    if ex.context == ""
        ctx = string("in ", ex.func)
    else
        ctx = string("in ", ex.func, ", in ", ex.context)
    end
    return string("TypeError: ", ctx, ", expected ", ex.expected, ", got ", targ)
end

# ArgumentError
function _showerror_str(ex::ArgumentError)
    return string("ArgumentError: ", ex.msg)
end

# EOFError
function _showerror_str(ex::EOFError)
    return "EOFError: read end of file"
end

# UndefKeywordError
function _showerror_str(ex::UndefKeywordError)
    return string("UndefKeywordError: keyword argument `", ex.var, "` not assigned")
end

# UndefVarError
# Issue #10318: when a scope is known (module-qualified lookup, e.g.
# `SomeModule.undefined_name`), keep the scope in the message so it matches
# upstream Julia 1.12's `not defined in `<scope>`` phrasing. A bare
# lookup (scope === nothing) prints `not defined` unchanged.
function _showerror_str(ex::UndefVarError)
    if ex.scope === nothing
        return string("UndefVarError: `", ex.var, "` not defined")
    else
        return string("UndefVarError: `", ex.var, "` not defined in `", ex.scope, "`")
    end
end

# MethodError
# Note: when raised from a Rust VmError::MethodError, ex.f holds the full
# error message string and ex.args is an empty tuple (Issue #8748/#8664).
function _showerror_str(ex::MethodError)
    args = ex.args
    if args === nothing
        # Fallback: MethodError constructed with Nothing args (legacy path).
        return string("MethodError: ", ex.f)
    end
    n = length(args)
    if n == 0 && ex.f isa AbstractString
        return string("MethodError: ", ex.f)
    end
    # A Function payload renders by name ("f", not "function f"),
    # matching upstream's `no method matching f(::T)` (Issue #11374).
    fname = ex.f isa Function ? string(nameof(ex.f)) : string(ex.f)
    result = string("MethodError: no method matching ", fname, "(")
    i = 1
    while i <= n
        if i > 1
            result = string(result, ", ")
        end
        result = string(result, "::", typeof(args[i]))
        i = i + 1
    end
    return string(result, ")")
end

# ParseError
function _showerror_str(ex::ParseError)
    return string("ParseError: ", ex.msg)
end

# SystemError
function _showerror_str(ex::SystemError)
    return string("SystemError: ", ex.prefix, ": errno ", ex.errnum)
end

# IOError
function _showerror_str(ex::IOError)
    return string("IOError: ", ex.msg, " (code ", ex.code, ")")
end

# LoadError
function _showerror_str(ex::LoadError)
    return string("LoadError: error at ", ex.file, ":", ex.line)
end

# MissingException
function _showerror_str(ex::MissingException)
    return string("MissingException: ", ex.msg)
end

# InvalidStateException
function _showerror_str(ex::InvalidStateException)
    return string("InvalidStateException: ", ex.msg, " (state: ", ex.state, ")")
end

# FieldError (Julia 1.12+)
# Mirrors julia/base/errorshow.jl `showerror(io, exc::FieldError)`.
# Upstream accesses `exc.type.name.wrapper` for the type name; here we use
# string(ex.type) as sjulia does not expose .name.wrapper (Issue #8664).
function _showerror_str(ex::FieldError)
    return string("FieldError: type ", ex.type, " has no field `", ex.field, "`")
end

# CanonicalIndexError
function _showerror_str(ex::CanonicalIndexError)
    return string("CanonicalIndexError: ", ex.func, " not defined for ", ex.arr_type)
end

# CapturedException - uses recursion
function _showerror_str(ex::CapturedException)
    base = string("CapturedException: ", _showerror_str(ex.ex))
    if ex.msg != ""
        return string(base, " (", ex.msg, ")")
    else
        return base
    end
end

# CompositeException
function _showerror_str(ex::CompositeException)
    n = length(ex.exceptions)
    base = string("CompositeException: ", n, " exception(s)")
    if n > 0
        first_ex = ex.exceptions[1]
        return string(base, ", first: ", string(first_ex))
    else
        return base
    end
end

# TaskFailedException
function _showerror_str(ex::TaskFailedException)
    if ex.task === nothing
        return "TaskFailedException"
    else
        t = ex.task
        if t._isexception && t.result !== nothing
            return string("TaskFailedException: nested task error: ", string(t.result))
        else
            return "TaskFailedException"
        end
    end
end

# ProcessFailedException
function _showerror_str(ex::ProcessFailedException)
    base = string("ProcessFailedException: exit code ", ex.exitcode)
    if ex.msg != ""
        return string(base, " (", ex.msg, ")")
    else
        return base
    end
end

# =============================================================================
# Public showerror API
# =============================================================================
# These functions write to the supplied IO stream. The old Issue #1217
# workaround printed to stdout and made `sprint(showerror, ex)` capture the
# wrong text after IOBuffer writes were fixed (Issue #9774).

"""
    showerror(io, e)

Show a descriptive representation of an exception object `e`.
This method is used to display the exception after a call to [`throw`](@ref).

`showerror(io, e)` writes to the provided IO stream, so `sprint(showerror, e)`
captures the same text that direct error display emits.

# Examples
```julia
julia> struct MyException <: Exception
           msg::String
       end

julia> err = ErrorException("test exception")
ErrorException("test exception")

julia> sprint_showerror(err)
"test exception"
```
"""
function showerror(io::IO, ex)
    print(io, _showerror_str(ex))
end

# Specialized versions call the internal string functions
function showerror(io::IO, ex::ErrorException)
    print(io, _showerror_str(ex))
end

function showerror(io::IO, ex::DimensionMismatch)
    print(io, _showerror_str(ex))
end

function showerror(io::IO, ex::KeyError)
    print(io, _showerror_str(ex))
end

function showerror(io::IO, ex::StringIndexError)
    print(io, _showerror_str(ex))
end

function showerror(io::IO, ex::BoundsError)
    print(io, _showerror_str(ex))
end

function showerror(io::IO, ex::OverflowError)
    print(io, _showerror_str(ex))
end

function showerror(io::IO, ex::StackOverflowError)
    print(io, _showerror_str(ex))
end

function showerror(io::IO, ex::OutOfMemoryError)
    print(io, _showerror_str(ex))
end

function showerror(io::IO, ex::UndefRefError)
    print(io, _showerror_str(ex))
end

function showerror(io::IO, ex::AssertionError)
    print(io, _showerror_str(ex))
end

function showerror(io::IO, ex::DivideError)
    print(io, _showerror_str(ex))
end

function showerror(io::IO, ex::DomainError)
    print(io, _showerror_str(ex))
end

function showerror(io::IO, ex::InexactError)
    print(io, _showerror_str(ex))
end

function showerror(io::IO, ex::TypeError)
    print(io, _showerror_str(ex))
end

function showerror(io::IO, ex::ArgumentError)
    print(io, _showerror_str(ex))
end

function showerror(io::IO, ex::EOFError)
    print(io, _showerror_str(ex))
end

function showerror(io::IO, ex::UndefKeywordError)
    print(io, _showerror_str(ex))
end

function showerror(io::IO, ex::UndefVarError)
    print(io, _showerror_str(ex))
end

function showerror(io::IO, ex::MethodError)
    print(io, _showerror_str(ex))
end

function showerror(io::IO, ex::ParseError)
    print(io, _showerror_str(ex))
end

function showerror(io::IO, ex::FieldError)
    print(io, _showerror_str(ex))
end

function showerror(io::IO, ex::SystemError)
    print(io, _showerror_str(ex))
end

function showerror(io::IO, ex::IOError)
    print(io, _showerror_str(ex))
end

function showerror(io::IO, ex::LoadError)
    print(io, _showerror_str(ex))
end

function showerror(io::IO, ex::MissingException)
    print(io, _showerror_str(ex))
end

function showerror(io::IO, ex::InvalidStateException)
    print(io, _showerror_str(ex))
end

function showerror(io::IO, ex::CanonicalIndexError)
    print(io, _showerror_str(ex))
end

function showerror(io::IO, ex::CapturedException)
    print(io, _showerror_str(ex))
end

function showerror(io::IO, ex::CompositeException)
    print(io, _showerror_str(ex))
end

function showerror(io::IO, ex::TaskFailedException)
    print(io, _showerror_str(ex))
end

function showerror(io::IO, ex::ProcessFailedException)
    print(io, _showerror_str(ex))
end

"""
    showerror(io, ex, bt; backtrace=true)

Show an exception with optional backtrace.

Note: SubsetJuliaVM doesn't have full backtrace support yet,
so this function currently ignores the backtrace argument.
"""
function showerror(io::IO, ex, bt)
    showerror(io, ex)
end

# =============================================================================
# sprint_showerror - helper to get error string
# =============================================================================

"""
    sprint_showerror(ex)

Return a string representation of the exception using `showerror`.

This helper returns the same string body used by `showerror(io, ex)`, without
allocating an `IOBuffer`.

# Examples
```julia
julia> sprint_showerror(ErrorException("something went wrong"))
"something went wrong"

julia> sprint_showerror(DimensionMismatch("dimensions must match"))
"DimensionMismatch: dimensions must match"
```
"""
function sprint_showerror(ex)
    return _showerror_str(ex)
end
