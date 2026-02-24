# =============================================================================
# errorshow.jl - Exception display with showerror
# =============================================================================
# Based on Julia's base/errorshow.jl
#
# This module implements the `showerror` function for customizing how
# exceptions are displayed. It provides default implementations for
# all exception types defined in error.jl.
#
# Note: SubsetJuliaVM has a known limitation where print(io, ...) does not
# write to IOBuffer (see Issue #1217). As a workaround, these implementations
# use string concatenation and the internal _showerror_str functions.

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
    else
        return string("BoundsError: attempt to access ", typeof(ex.a), " at index [", ex.i, "]")
    end
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
function _showerror_str(ex::InexactError)
    return string("InexactError: ", ex.func, "(", ex.T, ", ", ex.val, ")")
end

# TypeError
function _showerror_str(ex::TypeError)
    if ex.context == ""
        return string("TypeError: in ", ex.func, ", expected ", ex.expected, ", got ", typeof(ex.got))
    else
        return string("TypeError: in ", ex.func, ", in ", ex.context, ", expected ", ex.expected, ", got ", typeof(ex.got))
    end
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
function _showerror_str(ex::UndefVarError)
    return string("UndefVarError: `", ex.var, "` not defined")
end

# MethodError
function _showerror_str(ex::MethodError)
    args = ex.args
    n = length(args)
    result = string("MethodError: no method matching ", ex.f, "(")
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
    base = string("CompositeException: ", ex.count, " exception(s)")
    if ex.first_msg != ""
        return string(base, ", first: ", ex.first_msg)
    else
        return base
    end
end

# TaskFailedException
function _showerror_str(ex::TaskFailedException)
    if ex.msg == ""
        return "TaskFailedException"
    else
        return string("TaskFailedException: ", ex.msg)
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
# These functions write to an IO stream. Currently, they just print to stdout
# since print(io, ...) does not work with IOBuffer (Issue #1217).
# When Issue #1217 is fixed, these can use print(io, ...) directly.

"""
    showerror(io, e)

Show a descriptive representation of an exception object `e`.
This method is used to display the exception after a call to [`throw`](@ref).

Note: Due to a current limitation (Issue #1217), this prints to stdout
rather than the provided IO stream. Use `sprint_showerror` for string output.

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
    # Note: print(io, ...) currently writes to stdout, not the IOBuffer.
    # This is a known limitation (Issue #1217).
    print(_showerror_str(ex))
end

# Specialized versions call the internal string functions
function showerror(io::IO, ex::ErrorException)
    print(_showerror_str(ex))
end

function showerror(io::IO, ex::DimensionMismatch)
    print(_showerror_str(ex))
end

function showerror(io::IO, ex::KeyError)
    print(_showerror_str(ex))
end

function showerror(io::IO, ex::StringIndexError)
    print(_showerror_str(ex))
end

function showerror(io::IO, ex::BoundsError)
    print(_showerror_str(ex))
end

function showerror(io::IO, ex::OverflowError)
    print(_showerror_str(ex))
end

function showerror(io::IO, ex::StackOverflowError)
    print(_showerror_str(ex))
end

function showerror(io::IO, ex::OutOfMemoryError)
    print(_showerror_str(ex))
end

function showerror(io::IO, ex::UndefRefError)
    print(_showerror_str(ex))
end

function showerror(io::IO, ex::AssertionError)
    print(_showerror_str(ex))
end

function showerror(io::IO, ex::DivideError)
    print(_showerror_str(ex))
end

function showerror(io::IO, ex::DomainError)
    print(_showerror_str(ex))
end

function showerror(io::IO, ex::InexactError)
    print(_showerror_str(ex))
end

function showerror(io::IO, ex::TypeError)
    print(_showerror_str(ex))
end

function showerror(io::IO, ex::ArgumentError)
    print(_showerror_str(ex))
end

function showerror(io::IO, ex::EOFError)
    print(_showerror_str(ex))
end

function showerror(io::IO, ex::UndefKeywordError)
    print(_showerror_str(ex))
end

function showerror(io::IO, ex::UndefVarError)
    print(_showerror_str(ex))
end

function showerror(io::IO, ex::MethodError)
    print(_showerror_str(ex))
end

function showerror(io::IO, ex::ParseError)
    print(_showerror_str(ex))
end

function showerror(io::IO, ex::SystemError)
    print(_showerror_str(ex))
end

function showerror(io::IO, ex::IOError)
    print(_showerror_str(ex))
end

function showerror(io::IO, ex::LoadError)
    print(_showerror_str(ex))
end

function showerror(io::IO, ex::MissingException)
    print(_showerror_str(ex))
end

function showerror(io::IO, ex::InvalidStateException)
    print(_showerror_str(ex))
end

function showerror(io::IO, ex::CanonicalIndexError)
    print(_showerror_str(ex))
end

function showerror(io::IO, ex::CapturedException)
    print(_showerror_str(ex))
end

function showerror(io::IO, ex::CompositeException)
    print(_showerror_str(ex))
end

function showerror(io::IO, ex::TaskFailedException)
    print(_showerror_str(ex))
end

function showerror(io::IO, ex::ProcessFailedException)
    print(_showerror_str(ex))
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

This function works around the current IOBuffer limitation (Issue #1217)
by using internal string-based implementations.

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
