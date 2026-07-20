# =============================================================================
# error.jl - Exception types
# =============================================================================
# Based on Julia's base/boot.jl, base/array.jl, base/abstractdict.jl,
# and base/strings/string.jl
#
# These exception types mirror Julia's Base exception types.

# =============================================================================
# Abstract Exception Type
# =============================================================================

"""
    Exception

Abstract type for all exception types.
All exception structs should be subtypes of Exception.
"""
abstract type Exception end

# =============================================================================
# ErrorException Exception
# =============================================================================
# Defined in julia/base/boot.jl

"""
    ErrorException(msg)

Generic error type. The error message, in the `.msg` field, may provide more specific details.

# Examples
```julia
julia> error("something went wrong")
ERROR: something went wrong
```
"""
struct ErrorException <: Exception
    msg::AbstractString
end

# =============================================================================
# DimensionMismatch Exception
# =============================================================================
# Defined in julia/base/array.jl

"""
    DimensionMismatch([msg])

The objects called do not have matching dimensionality. Optional argument `msg` is a
descriptive error string.
"""
struct DimensionMismatch <: Exception
    msg::AbstractString
end
DimensionMismatch() = DimensionMismatch("")

# =============================================================================
# KeyError Exception
# =============================================================================
# Defined in julia/base/abstractdict.jl

"""
    KeyError(key)

An indexing operation into an `AbstractDict` (`Dict`) or `Set` like object tried to access or
delete a non-existent element.

Note: In official Julia, `key` has type `Any`. Here we use `String` as a workaround
for SubsetJuliaVM issue #361 (Cannot access untyped struct field containing String).
"""
struct KeyError <: Exception
    key::String
end

# =============================================================================
# StringIndexError Exception
# =============================================================================
# Defined in julia/base/strings/string.jl

"""
    StringIndexError(str, i)

An error occurred when trying to access `str` at index `i` that is not valid.
"""
struct StringIndexError <: Exception
    string::AbstractString
    index::Int
end

# =============================================================================
# FieldError Exception
# =============================================================================
# Defined in julia/base/boot.jl (Julia 1.12+)

"""
    FieldError(type::DataType, field::Symbol)

An attempt was made to access a field `field` that does not exist in `type`.

# Examples
```julia
julia> (x=1, y=2).z
ERROR: FieldError: type NamedTuple has no field `z`, available fields: `x`, `y`
```
"""
struct FieldError <: Exception
    type::DataType
    field::Symbol
end

# =============================================================================
# BoundsError Exception
# =============================================================================
# Defined in julia/base/boot.jl

"""
    BoundsError([a],[i])

An indexing operation into an array, `a`, tried to access an out-of-bounds element at index `i`.

# Examples
```julia
julia> A = fill(1.0, 7);
julia> A[8]
ERROR: BoundsError: attempt to access 7-element Vector{Float64} at index [8]
```
"""
struct BoundsError <: Exception
    a
    # Untyped like upstream boot.jl: the index may be an Int, a range
    # (`checkbounds(s, 2:4)`, Issue #10958), or a tuple of indices.
    i
end

# Constructor with just index
BoundsError(i::Int64) = BoundsError(nothing, i)

# =============================================================================
# OverflowError Exception
# =============================================================================
# Defined in julia/base/boot.jl

"""
    OverflowError([msg])

The result of an expression is too large for the specified type and will cause a wrap-around.

# Examples
```julia
julia> factorial(21)
ERROR: OverflowError: 21 is too large to look up in the table; consider using `factorial(big(21))` instead
```
"""
struct OverflowError <: Exception
    msg::AbstractString
end

# =============================================================================
# StackOverflowError Exception
# =============================================================================
# Defined in julia/base/boot.jl

"""
    StackOverflowError()

The function call grew beyond the size of the call stack. This usually happens when a call
recurses infinitely.
"""
struct StackOverflowError <: Exception end

# =============================================================================
# OutOfMemoryError Exception
# =============================================================================
# Defined in julia/base/boot.jl

"""
    OutOfMemoryError()

An operation allocated too much memory for either the system or the garbage collector to
handle properly.
"""
struct OutOfMemoryError <: Exception end

# =============================================================================
# UndefRefError Exception
# =============================================================================
# Defined in julia/base/boot.jl

"""
    UndefRefError()

The item or field is not defined for the given object.
"""
struct UndefRefError <: Exception end

# =============================================================================
# AssertionError Exception
# =============================================================================
# Defined in julia/base/boot.jl

"""
    AssertionError([msg])

The asserted condition did not evaluate to `true`.
Optional argument `msg` is a descriptive error string.

# Examples
```julia
julia> @assert false
ERROR: AssertionError: ...
```
"""
struct AssertionError <: Exception
    msg::AbstractString
end
AssertionError() = AssertionError("")

# =============================================================================
# DivideError Exception
# =============================================================================
# Defined in julia/base/boot.jl

"""
    DivideError()

Integer division was attempted with a denominator value of 0.

# Examples
```julia
julia> div(1, 0)
ERROR: DivideError: integer division error
```
"""
struct DivideError <: Exception end

# =============================================================================
# DomainError Exception
# =============================================================================
# Defined in julia/base/boot.jl

"""
    DomainError(val)
    DomainError(val, msg)

The argument `val` to a function or constructor is outside the valid domain.

# Examples
```julia
julia> sqrt(-1)
ERROR: DomainError with -1.0:
sqrt was called with a negative real argument but will only return a complex result if called with a complex argument.
```
"""
struct DomainError <: Exception
    val
    msg::AbstractString
end
DomainError(val) = DomainError(val, "")

# =============================================================================
# InexactError Exception
# =============================================================================
# Defined in julia/base/boot.jl

"""
    InexactError(name::Symbol, args...)

Cannot exactly convert in a method of function `name`.

# Examples
```julia
julia> Int(3.14)
ERROR: InexactError: Int64(3.14)
```
"""
struct InexactError <: Exception
    func::Symbol
    args
end

InexactError(name::Symbol, args...) = InexactError(name, args)

# =============================================================================
# TypeError Exception
# =============================================================================
# Defined in julia/base/boot.jl

"""
    TypeError(func::Symbol, context, expected::Type, got)

A type assertion failure, or calling an intrinsic function with an incorrect argument type.

# Examples
```julia
julia> "foo"::Int
ERROR: TypeError: in typeassert, expected Int64, got a value of type String
```

Note: In official Julia, `context` has type `Union{AbstractString,GlobalRef,Symbol}`.
Here we use `AbstractString` as SubsetJuliaVM does not support Union types in struct fields.
"""
struct TypeError <: Exception
    func::Symbol
    context::AbstractString
    expected::Type
    got
end

# =============================================================================
# ArgumentError Exception
# =============================================================================
# Defined in julia/base/boot.jl

"""
    ArgumentError(msg)

The parameters to a function call do not match a valid signature.
Argument `msg` is a descriptive error string.

# Examples
```julia
julia> sqrt(-1)
ERROR: DomainError ...
```
"""
struct ArgumentError <: Exception
    msg::AbstractString
end

# =============================================================================
# EOFError Exception
# =============================================================================
# Defined in julia/base/io.jl

"""
    EOFError()

No more data was available to read from a file or stream.
"""
struct EOFError <: Exception end

# =============================================================================
# UndefKeywordError Exception
# =============================================================================
# Defined in julia/base/boot.jl

"""
    UndefKeywordError(var::Symbol)

The required keyword argument `var` was not assigned in a function call.

# Examples
```julia
julia> f(; x) = x
f (generic function with 1 method)

julia> f()
ERROR: UndefKeywordError: keyword argument `x` not assigned
```
"""
struct UndefKeywordError <: Exception
    var::Symbol
end

# =============================================================================
# UndefVarError Exception
# =============================================================================
# Defined in julia/base/boot.jl

"""
    UndefVarError(var::Symbol, [scope])

A symbol in the current scope is not defined.

`scope` (Issue #10318) is the scope the binding was looked up in — a module
path string such as `Main.SomeModule` (or `nothing` for a bare local/global
lookup). Upstream Julia 1.12 carries the scope as a `Module`/`Symbol`; the
subset uses a string (or `nothing`) rendered by `showerror` as
`not defined in \`<scope>\``.

# Examples
```julia
julia> a
ERROR: UndefVarError: `a` not defined in `Main`
```
"""
struct UndefVarError <: Exception
    var::Symbol
    scope
end
# Bare lookup with no known scope (matches the 1-arg upstream constructor).
UndefVarError(var::Symbol) = UndefVarError(var, nothing)

# =============================================================================
# MethodError Exception
# =============================================================================
# Defined in julia/base/boot.jl

"""
    MethodError(f, args)

A method with the required type signature does not exist in the given generic function.
Alternatively, there is no unique most-specific method.

# Examples
```julia
julia> abs("Not a number")
ERROR: MethodError: no method matching abs(::String)
```

Note: In official Julia, MethodError also has a `world` field for world age.
Here we use a simplified version without world age tracking.
"""
struct MethodError <: Exception
    f
    args
end

# =============================================================================
# ParseError Exception
# =============================================================================
# Defined in julia/base/meta.jl

# Structured parser detail types carried by ParseError.detail. Upstream owns
# these under Base.JuliaSyntax; sjulia preserves the JuliaSyntax owner here,
# while the independently tracked Base-qualified submodule binding gap remains
# Issue #11614.
module JuliaSyntax
struct SourceFile
    code::AbstractString
    byte_offset::Int
    filename
    first_line::Int
    line_starts::Vector{Int}
end

struct Diagnostic
    first_byte::Int
    last_byte::Int
    level::Symbol
    message::String
end

struct ParseError
    source::SourceFile
    diagnostics::Vector{Diagnostic}
    incomplete_tag::Symbol
end
end

"""
    ParseError(msg)
    ParseError(msg, detail)

The argument `msg` describes a syntax error in a string.

# Examples
```julia
julia> Meta.parse("1 +")
ERROR: ParseError:
# Error @ none:1:4
1 +
#  └ ── Expected `end`
```

The `detail` field carries structured `JuliaSyntax` source and diagnostics for
parser-raised errors. The one-argument constructor retains Julia's documented
`nothing` default for manually constructed errors.
"""
struct ParseError <: Exception
    msg::AbstractString
    detail
    # Workaround: keep the Base-owned two-field constructor explicit so a same-leaf JuliaSyntax.ParseError does not hide the cached default. (Issue #10445)
    ParseError(msg::AbstractString, detail) = new(msg, detail)
end
ParseError(msg::AbstractString) = ParseError(msg, nothing)

# =============================================================================
# SystemError Exception
# =============================================================================
# Defined in julia/base/io.jl

"""
    SystemError(prefix::AbstractString, errnum::Integer)

A system call failed with an error code `errnum`.

# Examples
```julia
julia> open("/nonexistent/file")
ERROR: SystemError: opening file "/nonexistent/file": No such file or directory
```

Note: In official Julia, SystemError also has an `extrainfo` field.
Here we use a simplified version without the extra info field.
"""
struct SystemError <: Exception
    prefix::AbstractString
    errnum::Int64
end

# =============================================================================
# IOError Exception
# =============================================================================
# Defined in julia/base/libuv.jl

"""
    IOError(msg::AbstractString, code::Integer)

I/O operation failed with the specified message and error code.

# Examples
```julia
julia> read("/nonexistent")
ERROR: IOError: open("/nonexistent"): no such file or directory (ENOENT)
```
"""
struct IOError <: Exception
    msg::AbstractString
    code::Int64
end

# =============================================================================
# WrappedException Abstract Type
# =============================================================================
# Defined in julia/base/boot.jl

"""
    WrappedException

Abstract type for exceptions that wrap another exception.
`LoadError` and `InitError` are subtypes of this.
"""
abstract type WrappedException <: Exception end

# =============================================================================
# LoadError Exception
# =============================================================================
# Defined in julia/base/boot.jl

"""
    LoadError(file::AbstractString, line::Int, error)

An error occurred while `include`ing, `require`ing, or `using` a file.

# Examples
```julia
julia> include("nonexistent.jl")
ERROR: LoadError: could not open file "nonexistent.jl"
```
"""
struct LoadError <: WrappedException
    file::AbstractString
    line::Int64
    error
end

# =============================================================================
# error function
# =============================================================================
# Based on julia/base/error.jl
#
# The error function is the primary way to raise an ErrorException.

"""
    error(message::AbstractString)

Raise an `ErrorException` with the given message.

# Examples
```julia
julia> error("something went wrong")
ERROR: something went wrong
```
"""
error(s::AbstractString) = throw(ErrorException(s))

"""
    error()

Raise an empty `ErrorException`.
"""
error() = throw(ErrorException(""))

"""
    error(msg...)

Raise an `ErrorException` with a message constructed by `string(msg...)`.

# Examples
```julia
julia> error("x = ", 42)
ERROR: x = 42
```
"""
function error(a, b)
    throw(ErrorException(string(a, b)))
end

function error(a, b, c)
    throw(ErrorException(string(a, b, c)))
end

function error(a, b, c, d)
    throw(ErrorException(string(a, b, c, d)))
end

# =============================================================================
# rethrow function (documented stub - actual implementation is in VM)
# =============================================================================
# Based on julia/base/error.jl
#
# The rethrow function re-throws the current exception from within a catch block.
# Note: The actual implementation is in the VM (Instr::RethrowCurrent/RethrowOther).

"""
    rethrow()

Rethrow the current exception from within a `catch` block. The rethrown
exception will continue propagation as if it had not been caught.

# Examples
```julia
try
    error("original error")
catch e
    println("caught: ", e)
    rethrow()  # re-throw the original error
end
```
"""
function rethrow end  # Implemented as VM builtin

"""
    rethrow(e)

Rethrow with an alternative exception object `e`.

Note: This misrepresents the program state at the time of the error so you're
encouraged to instead throw a new exception using `throw(e)`.

# Examples
```julia
try
    error("original error")
catch
    rethrow(ErrorException("replaced error"))
end
```
"""
function rethrow(e) end  # Implemented as VM builtin

# =============================================================================
# systemerror function
# =============================================================================
# Based on julia/base/error.jl
#
# The systemerror function raises a SystemError when a condition is true.

"""
    systemerror(sysfunc, iftrue::Bool)

Raises a `SystemError` for `sysfunc` if `iftrue` is `true`.

# Examples
```julia
systemerror("opening file", !isfile(path))
```
"""
function systemerror(p::AbstractString, b::Bool)
    if b
        throw(SystemError(p, 0))
    end
    nothing
end

"""
    systemerror(sysfunc, errno::Int64)

Raises a `SystemError` for `sysfunc` with the given error number.

# Examples
```julia
systemerror("syscall failed", 1)  # EPERM
```
"""
function systemerror(p::AbstractString, errno::Int64)
    throw(SystemError(p, errno))
end

"""
    systemerror(sysfunc)

Raises a `SystemError` for `sysfunc` with errno 0.
"""
function systemerror(p::AbstractString)
    throw(SystemError(p, 0))
end

# =============================================================================
# MissingException
# =============================================================================
# Based on julia/base/missing.jl

"""
    MissingException(msg)

Exception thrown when a `missing` value is encountered in a situation
where it is not supported. The error message, in the `msg` field
may provide more specific details.

# Examples
```julia
julia> throw(MissingException("cannot perform operation on missing"))
ERROR: MissingException: cannot perform operation on missing
```
"""
struct MissingException <: Exception
    msg::AbstractString
end

# =============================================================================
# InvalidStateException
# =============================================================================
# Based on julia/base/channels.jl

"""
    InvalidStateException(msg, state)

Exception thrown when an operation cannot be performed due to
the object being in an invalid state. The error message (`msg`)
describes what operation was attempted, and `state` indicates
the current state of the object.

# Examples
```julia
julia> throw(InvalidStateException("cannot write to closed channel", :closed))
ERROR: InvalidStateException: cannot write to closed channel
```
"""
struct InvalidStateException <: Exception
    msg::String
    state::Symbol
end

# =============================================================================
# CanonicalIndexError Exception
# =============================================================================

"""
    CanonicalIndexError

Indicates that a canonical `getindex` or `setindex!` method was called
instead of the expected implementation.

This is typically thrown when an AbstractArray subtype does not implement
the required indexing methods.

Note: In official Julia, the `type` field has type `Any`.
Here we use a simplified version where `arr_type` is String (renamed from `type` to avoid keyword conflict).
"""
struct CanonicalIndexError <: Exception
    func::String
    arr_type::String
end

# =============================================================================
# CapturedException Exception
# =============================================================================

"""
    CapturedException

Container for a captured exception and its backtrace. Can be serialized.

When a task fails, its exception is captured along with its backtrace
for later retrieval and display.

Note: In official Julia, `processed_bt` stores processed backtrace information.
Here we use a simplified version with just the exception and a message.
"""
struct CapturedException <: Exception
    ex
    msg::AbstractString
end

# Simple constructor
CapturedException(ex) = CapturedException(ex, "")

# =============================================================================
# CompositeException Exception
# =============================================================================

"""
    CompositeException

Wrap a `Vector` of exceptions thrown by multiple tasks.

For example, if a group of workers are executing several tasks, and multiple
workers fail, the resulting CompositeException will contain information from
each worker indicating where and why the exception(s) occurred.
"""
mutable struct CompositeException <: Exception
    exceptions::Vector{Any}
end

# Default constructor
CompositeException() = CompositeException(Any[])

length(c::CompositeException) = length(c.exceptions)
isempty(c::CompositeException) = isempty(c.exceptions)
iterate(c::CompositeException) = iterate(c.exceptions)
iterate(c::CompositeException, state) = iterate(c.exceptions, state)

# =============================================================================
# TaskFailedException Exception
# =============================================================================

"""
    TaskFailedException

This exception is thrown by a `wait(t)` call when task `t` fails.
Wraps the failed task so callers can inspect it.

Note: The `task` field is typed `Any` because `Task` is defined in task.jl
which is loaded after this file.
"""
struct TaskFailedException <: Exception
    task::Any  # actually Task, but Task not yet defined here
end

# Default no-arg constructor
TaskFailedException() = TaskFailedException(nothing)

# =============================================================================
# ProcessFailedException Exception
# =============================================================================

"""
    ProcessFailedException

Indicates problematic exit status of a process.

When running commands or pipelines, this is thrown to indicate
a nonzero exit code was returned (i.e. that the invoked process failed).

Note: In official Julia, this contains a `Vector{Process}`.
Here we use a simplified version with the exit code.
"""
struct ProcessFailedException <: Exception
    exitcode::Int64
    msg::AbstractString
end

# Constructor with default message
ProcessFailedException(exitcode::Int64) = ProcessFailedException(exitcode, "")

# =============================================================================
# ExponentialBackOff Iterator
# =============================================================================
# Based on julia/base/error.jl
#
# An iterator for exponential backoff delays, used with retry().

"""
    ExponentialBackOff(; n=1, first_delay=0.05, max_delay=10.0, factor=5.0, jitter=0.1)

A [`Float64`](@ref) iterator of length `n` whose elements exponentially increase at a
rate in the interval `factor` * (1 ± `jitter`).  The first element is
`first_delay` and all elements are clamped to `max_delay`.

# Examples
```julia
for delay in ExponentialBackOff(n=3, first_delay=1.0)
    println(delay)
    sleep(delay)
end
```
"""
struct ExponentialBackOff
    n::Int64
    first_delay::Float64
    max_delay::Float64
    factor::Float64
    jitter::Float64
end

# Keyword constructor with defaults
function ExponentialBackOff(; n=1, first_delay=0.05, max_delay=10.0, factor=5.0, jitter=0.1)
    # Convert to proper types
    n_int = convert(Int64, n)
    fd = convert(Float64, first_delay)
    md = convert(Float64, max_delay)
    f = convert(Float64, factor)
    j = convert(Float64, jitter)
    ExponentialBackOff(n_int, fd, md, f, j)
end

# Iterator interface for ExponentialBackOff
function iterate(ebo::ExponentialBackOff)
    ebo.n < 1 && return nothing
    curr_delay = min(ebo.first_delay, ebo.max_delay)
    # State is (remaining_iterations, next_delay)
    next_delay = min(ebo.max_delay, curr_delay * ebo.factor * (1.0 - ebo.jitter + (rand() * 2.0 * ebo.jitter)))
    return (curr_delay, (ebo.n - 1, next_delay))
end

function iterate(ebo::ExponentialBackOff, state)
    # State is a tuple (remaining_iterations, curr_delay)
    remaining = state[1]
    curr_delay = state[2]
    remaining < 1 && return nothing
    next_delay = min(ebo.max_delay, curr_delay * ebo.factor * (1.0 - ebo.jitter + (rand() * 2.0 * ebo.jitter)))
    return (curr_delay, (remaining - 1, next_delay))
end

length(ebo::ExponentialBackOff) = ebo.n
# Note: eltype(::Type{ExponentialBackOff}) = Float64 is not implemented as
# type-parameterized eltype is not yet supported in SubsetJuliaVM

"""
    retry(f; delays=ExponentialBackOff(), check=nothing) -> Function

Return a function that calls `f`. If `f` throws, call it again after each
delay while `check` is `nothing` or returns `true`.
"""
function retry(f; delays=ExponentialBackOff(), check=nothing)
    return (args...; kwargs...) -> begin
        y = iterate(delays)
        while y !== nothing
            delay = y[1]
            state = y[2]
            try
                return f(args...; kwargs...)
            catch err
                if check !== nothing
                    result = check(state, err)
                    if length(result) == 2
                        state = result[1]
                        retry_or_not = result[2]
                    else
                        retry_or_not = result
                    end
                    if !retry_or_not
                        rethrow()
                    end
                end
            end
            sleep(delay)
            y = iterate(delays, state)
        end
        return f(args...; kwargs...)
    end
end

# =============================================================================
# Backtrace Functions
# =============================================================================
# SubsetJuliaVM exposes VM frame snapshots as Vector{String}. This is not yet
# Julia's StackFrame object model, but it preserves function and source-line
# information for user code that inspects non-empty backtraces.

"""
    backtrace()

Get a backtrace object for the current program point.

Note: In SubsetJuliaVM, this returns a vector of rendered VM stack frames.

# Examples
```julia
bt = backtrace()
```
"""
function backtrace()
    return _sjulia_backtrace()
end

"""
    catch_backtrace()

Get the backtrace of the current exception, for use within `catch` blocks.

Note: In SubsetJuliaVM, this returns the rendered VM stack frames captured for
the current exception.

# Examples
```julia
try
    error("oops")
catch
    bt = catch_backtrace()
end
```
"""
function catch_backtrace()
    return _sjulia_catch_backtrace()
end

"""
    current_exceptions(; backtrace::Bool=true)

Get the stack of exceptions currently being handled.

Note: In SubsetJuliaVM, this returns an empty array as exception stack
tracking is not yet fully implemented.

# Examples
```julia
try
    error("outer")
catch
    try
        error("inner")
    catch
        excs = current_exceptions()  # Returns empty array
    end
end
```
"""
function current_exceptions(; backtrace::Bool=true)
    return []  # Stub: return empty exception stack
end

"""
    stacktrace()

Get a stack trace in a more user-friendly format than `backtrace()`.

Note: In SubsetJuliaVM, this returns a vector of rendered VM stack frames.

# Examples
```julia
st = stacktrace()
```
"""
function stacktrace()
    return _sjulia_stacktrace()
end

"""
    stacktrace(trace)

Returns stack frame information from the given backtrace.

Note: In SubsetJuliaVM, backtrace objects are already rendered frame strings,
so this returns `trace` unchanged.
"""
function stacktrace(trace)
    return _sjulia_stacktrace(trace)
end
