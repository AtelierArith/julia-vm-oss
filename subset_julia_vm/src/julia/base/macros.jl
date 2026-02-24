# =============================================================================
# macros.jl - Base Macros
# =============================================================================
# User-defined macros that extend the built-in macro system.
#
# Note: Core macros like @assert, @show, @time are still implemented in Rust
# because they require features not yet available in the macro system
# (e.g., string(expr) for getting expression representation).
#
# This file demonstrates that user-defined macros can be defined in Julia
# and loaded as part of the base library.

# =============================================================================
# @inline - Hint for inlining (currently no-op, for compatibility)
# =============================================================================
macro inline(ex)
    esc(ex)
end

# =============================================================================
# @noinline - Hint to prevent inlining (currently no-op, for compatibility)
# =============================================================================
macro noinline(ex)
    esc(ex)
end

# =============================================================================
# @inbounds - Disable bounds checking (currently no-op, for compatibility)
# =============================================================================
# In full Julia, @inbounds disables bounds checking within the expression.
# Here we just return the expression unchanged since SubsetJuliaVM always
# performs bounds checking for safety.
macro inbounds(ex)
    esc(ex)
end

# =============================================================================
# @nospecialize - Hint to avoid specialization (currently no-op, for compatibility)
# =============================================================================
# In full Julia, @nospecialize prevents the compiler from specializing
# a method for specific argument types. Since SubsetJuliaVM does not
# perform type specialization, this is a no-op.
macro nospecialize(ex)
    esc(ex)
end

# =============================================================================
# @simd - SIMD vectorization hint (currently no-op, for compatibility)
# =============================================================================
# In full Julia, @simd allows the compiler to use SIMD instructions for
# the annotated for loop. Since SubsetJuliaVM does not perform SIMD
# optimization, this is a no-op.
macro simd(ex)
    esc(ex)
end

# =============================================================================
# @boundscheck - Mark bounds-checking code (currently no-op, for compatibility)
# =============================================================================
# In full Julia, @boundscheck marks code that should be skipped when @inbounds
# is active. Since we always check bounds, this is a no-op.
macro boundscheck(ex)
    esc(ex)
end

# =============================================================================
# @propagate_inbounds - Propagate inbounds context (currently no-op, for compatibility)
# =============================================================================
# In full Julia, this is used to propagate @inbounds from caller to callee.
# Since we always check bounds, this is a no-op.
macro propagate_inbounds(ex)
    esc(ex)
end

# =============================================================================
# @label and @goto - Low-level control flow
# =============================================================================
# NOTE: @label and @goto are implemented as built-in compiler macros in the
# lowering phase (lowering/stmt/macros.rs). They are NOT defined here because
# they require special handling to be lowered to Stmt::Label and Stmt::Goto IR
# nodes, which cannot be done through normal macro expansion.
#
# Usage:
#   @goto myloop      # Jump to @label myloop
#   @label myloop     # Define jump target named myloop
#
# Example:
#   i = 0
#   @label start
#   i += 1
#   if i < 10
#       @goto start
#   end
#   println(i)  # prints 10
#
# WARNING: Use @goto sparingly. In most cases, while/for loops provide
# clearer control flow. @goto is useful for breaking out of deeply nested
# loops or implementing state machines.

# =============================================================================
# @eval - Evaluate expression at compile time (simplified)
# =============================================================================
# In full Julia, @eval would evaluate the expression in the module scope.
# Workaround: returns expression as-is (no compile-time eval).
macro eval(ex)
    esc(ex)
end

# =============================================================================
# @deprecate - Mark function as deprecated (simplified, just evaluates)
# =============================================================================
macro deprecate(old, new)
    esc(old)
end

# =============================================================================
# @debug, @info, @warn, @error - Logging macros (simplified)
# =============================================================================
# Based on Julia's base/logging/logging.jl
# This is a simplified implementation that prints to console without
# the full logging infrastructure (LogLevel, AbstractLogger, etc.)
#
# Usage:
#   @debug "Debug message"
#   @info "Information message"
#   @warn "Warning message"
#   @error "Error message"
#
#   # With key=value pairs:
#   @info "Processing" x=10 y=20
#
# Note: Unlike full Julia, this simplified version:
# - Always prints all log levels (no filtering)
# - Does not support logger backends
# - Does not include source file/line information

# Helper functions for logging
# These use fixed-arity functions to avoid issues with array literals in macros

function _log_msg_0(level::String, msg)
    prefix = ""
    if level == "Debug"
        prefix = "┌ Debug: "
    elseif level == "Info"
        prefix = "┌ Info: "
    elseif level == "Warning"
        prefix = "┌ Warning: "
    elseif level == "Error"
        prefix = "┌ Error: "
    end
    println(prefix, msg)
    println("└")
    nothing
end

function _log_msg_1(level::String, msg, k1, v1)
    prefix = ""
    if level == "Debug"
        prefix = "┌ Debug: "
    elseif level == "Info"
        prefix = "┌ Info: "
    elseif level == "Warning"
        prefix = "┌ Warning: "
    elseif level == "Error"
        prefix = "┌ Error: "
    end
    println(prefix, msg)
    println("│   ", k1, " = ", v1)
    println("└")
    nothing
end

function _log_msg_2(level::String, msg, k1, v1, k2, v2)
    prefix = ""
    if level == "Debug"
        prefix = "┌ Debug: "
    elseif level == "Info"
        prefix = "┌ Info: "
    elseif level == "Warning"
        prefix = "┌ Warning: "
    elseif level == "Error"
        prefix = "┌ Error: "
    end
    println(prefix, msg)
    println("│   ", k1, " = ", v1)
    println("│   ", k2, " = ", v2)
    println("└")
    nothing
end

function _log_msg_3(level::String, msg, k1, v1, k2, v2, k3, v3)
    prefix = ""
    if level == "Debug"
        prefix = "┌ Debug: "
    elseif level == "Info"
        prefix = "┌ Info: "
    elseif level == "Warning"
        prefix = "┌ Warning: "
    elseif level == "Error"
        prefix = "┌ Error: "
    end
    println(prefix, msg)
    println("│   ", k1, " = ", v1)
    println("│   ", k2, " = ", v2)
    println("│   ", k3, " = ", v3)
    println("└")
    nothing
end

# Debug level
macro debug(msg)
    quote
        _log_msg_0("Debug", $(esc(msg)))
    end
end

macro debug(msg, kw1)
    k1 = string(kw1.args[1])
    v1 = kw1.args[2]
    quote
        _log_msg_1("Debug", $(esc(msg)), $k1, $(esc(v1)))
    end
end

macro debug(msg, kw1, kw2)
    k1 = string(kw1.args[1])
    v1 = kw1.args[2]
    k2 = string(kw2.args[1])
    v2 = kw2.args[2]
    quote
        _log_msg_2("Debug", $(esc(msg)), $k1, $(esc(v1)), $k2, $(esc(v2)))
    end
end

# Info level
macro info(msg)
    quote
        _log_msg_0("Info", $(esc(msg)))
    end
end

macro info(msg, kw1)
    k1 = string(kw1.args[1])
    v1 = kw1.args[2]
    quote
        _log_msg_1("Info", $(esc(msg)), $k1, $(esc(v1)))
    end
end

macro info(msg, kw1, kw2)
    k1 = string(kw1.args[1])
    v1 = kw1.args[2]
    k2 = string(kw2.args[1])
    v2 = kw2.args[2]
    quote
        _log_msg_2("Info", $(esc(msg)), $k1, $(esc(v1)), $k2, $(esc(v2)))
    end
end

macro info(msg, kw1, kw2, kw3)
    k1 = string(kw1.args[1])
    v1 = kw1.args[2]
    k2 = string(kw2.args[1])
    v2 = kw2.args[2]
    k3 = string(kw3.args[1])
    v3 = kw3.args[2]
    quote
        _log_msg_3("Info", $(esc(msg)), $k1, $(esc(v1)), $k2, $(esc(v2)), $k3, $(esc(v3)))
    end
end

# Warn level
macro warn(msg)
    quote
        _log_msg_0("Warning", $(esc(msg)))
    end
end

macro warn(msg, kw1)
    k1 = string(kw1.args[1])
    v1 = kw1.args[2]
    quote
        _log_msg_1("Warning", $(esc(msg)), $k1, $(esc(v1)))
    end
end

macro warn(msg, kw1, kw2)
    k1 = string(kw1.args[1])
    v1 = kw1.args[2]
    k2 = string(kw2.args[1])
    v2 = kw2.args[2]
    quote
        _log_msg_2("Warning", $(esc(msg)), $k1, $(esc(v1)), $k2, $(esc(v2)))
    end
end

# Error level (note: different from Base.error() which throws an exception)
macro error(msg)
    quote
        _log_msg_0("Error", $(esc(msg)))
    end
end

macro error(msg, kw1)
    k1 = string(kw1.args[1])
    v1 = kw1.args[2]
    quote
        _log_msg_1("Error", $(esc(msg)), $k1, $(esc(v1)))
    end
end

macro error(msg, kw1, kw2)
    k1 = string(kw1.args[1])
    v1 = kw1.args[2]
    k2 = string(kw2.args[1])
    v2 = kw2.args[2]
    quote
        _log_msg_2("Error", $(esc(msg)), $k1, $(esc(v1)), $k2, $(esc(v2)))
    end
end

# =============================================================================
# @doc - Documentation macro (no-op; doc system not implemented)
# =============================================================================
macro doc(str, ex)
    esc(ex)
end

# =============================================================================
# @macroexpand - Show the expanded form of a macro call
# =============================================================================
# This macro is implemented in Rust lowering as it needs to intercept
# macro expansion and return the result as an Expr value.
# See: src/lowering/expr/mod.rs and src/lowering/stmt/macros.rs

# =============================================================================
# @generated - Generated functions (Phase 1: Fallback execution only)
# =============================================================================
# In Julia, @generated functions allow code generation based on argument types.
# Since SubsetJuliaVM is an AOT compiler without JIT, we implement Phase 1:
# - `if @generated ... else fallback end` pattern executes the fallback
# - Pure `@generated function ... end` is not supported (requires JIT)
#
# This macro returns a special marker that lowering detects and handles.
# The lowering phase will:
# 1. Detect `if @generated() ... else ... end` pattern
# 2. Extract only the `else` branch (fallback code)
# 3. Compile and execute the fallback
#
# Usage:
#   function f(x::T) where T
#       if @generated
#           :(x^2)        # Ignored in Phase 1
#       else
#           x^2           # This is executed
#       end
#   end
#
# See: docs/vm/GENERATED_FUNCTION_PLAN.md for full implementation plan.

macro generated()
    # Return a special marker expression that lowering will detect
    # In Julia, this would be Expr(:generated)
    # We use a call to a sentinel function that lowering recognizes
    Expr(:call, :__generated_marker__)
end

macro generated(f)
    # Transform @generated function into if @generated ... else ... end pattern
    # This is the full @generated function syntax:
    #   @generated function f(x) ... end
    #
    # For Phase 1, we reject this syntax since it requires the generated code
    # to actually run. Users should use the if @generated ... else ... end pattern.
    error("@generated function syntax is not supported in SubsetJuliaVM Phase 1. Use `if @generated ... else fallback end` pattern instead.")
end

# =============================================================================
# @evalpoly - Evaluate polynomial using Horner's method
# =============================================================================
# @evalpoly(z, c0, c1, c2, ...) evaluates c0 + c1*z + c2*z^2 + ...
# This is a Pure Julia implementation using splat interpolation.
#
# In official Julia, this is implemented as:
#   macro evalpoly(z, p...)
#       zesc, pesc = esc(z), esc.(p)
#       :(evalpoly($zesc, ($(pesc...),)))
#   end
#
# Since we don't have broadcast on tuples (esc.(p)), we use a simplified
# version without explicit esc(). This works because our hygiene system
# handles variable renaming automatically.
#
# Usage:
#   @evalpoly(2, 1, 2, 3)  # = 1 + 2*2 + 3*2^2 = 1 + 4 + 12 = 17
macro evalpoly(z, p...)
    quote
        evalpoly($z, ($(p...),))
    end
end

# =============================================================================
# @something - Return first non-nothing value
# =============================================================================
# @something(a, b, c, ...) returns the first argument that is not `nothing`.
# If all arguments are `nothing`, returns the last argument (which is `nothing`).
#
# This macro expands to a call to the `something` function.
#
# Usage:
#   @something(nothing, 42)        # => 42
#   @something(nothing, nothing, 1)  # => 1
#   @something(1, 2, 3)            # => 1
macro something(args...)
    # Expand to: something(arg1, arg2, ...)
    :(something($(args...)))
end

# =============================================================================
# @coalesce - Return first non-missing value
# =============================================================================
# @coalesce(a, b, c, ...) returns the first argument that is not `missing`.
# If all arguments are `missing`, returns the last argument (which is `missing`).
#
# This macro expands to a call to the `coalesce` function.
# Similar to @something but for `missing` instead of `nothing`.
#
# Usage:
#   @coalesce(missing, 42)        # => 42
#   @coalesce(missing, missing, 1)  # => 1
#   @coalesce(1, 2, 3)            # => 1
macro coalesce(args...)
    # Expand to: coalesce(arg1, arg2, ...)
    :(coalesce($(args...)))
end

# =============================================================================
# @assert - Assert that a condition is true
# =============================================================================
# @assert(condition) throws AssertionError if condition is false.
# @assert(condition, msg) uses custom message.
#
# Usage:
#   @assert(1 + 1 == 2)           # passes
#   @assert(false)                # throws AssertionError
#   @assert(false, "custom msg")  # throws AssertionError with custom message

# Helper function for @assert macro
# condition can be any type that is truthy/falsy
function _do_assert(condition, msg::String)
    if !condition
        throw(AssertionError(msg))
    end
    nothing
end

macro assert(ex)
    # Expand to: _do_assert(condition, "") — AssertionError with empty message
    # Official Julia: @assert false throws AssertionError("") not AssertionError("AssertionError")
    :(_do_assert($(esc(ex)), ""))
end

macro assert(ex, msg)
    # Expand to: _do_assert(condition, msg)
    :(_do_assert($(esc(ex)), $(esc(msg))))
end

# =============================================================================
# @show - Print expression and its value
# =============================================================================
# @show(expr) prints "expr = value" and returns value.
#
# Usage:
#   @show(1 + 2)  # prints "1 + 2 = 3" and returns 3
#   x = 5
#   @show(x)      # prints "x = 5" and returns 5

# Helper function for @show macro
function _do_show(expr_str::String, value)
    println(expr_str, " = ", value)
    value
end

macro show(ex)
    # Get expression as string at macro expansion time
    expr_str = string(ex)
    # Use quote...end syntax to ensure expr_str is evaluated at expansion time
    # (see issue #1352: :(...) syntax causes string(ex) to become runtime code)
    quote
        _do_show($expr_str, $(esc(ex)))
    end
end

# =============================================================================
# gensym - Generate unique symbol names (Rust builtin)
# =============================================================================
# gensym() generates a unique symbol like ##123
# gensym("tag") generates a unique symbol like ##tag#123
#
# Used for macro hygiene to create unique variable names that won't
# conflict with user-defined variables.
#
# Note: gensym is implemented as a Rust builtin because it requires
# global mutable state, which is not yet fully supported in Pure Julia prelude.
#
# Usage:
#   gensym()        # => Symbol("##1")
#   gensym("x")     # => Symbol("##x#2")
#   gensym("loop")  # => Symbol("##loop#3")

# =============================================================================
# @view - Create a view of an array slice
# =============================================================================
# Based on Julia's base/views.jl
#
# @view A[i:j] transforms the indexing expression into view(A, i:j)
# which creates a SubArray (a view) instead of copying the data.
#
# Usage:
#   v = @view arr[2:4]     # v is a view, not a copy
#   v[1] = 10              # modifies arr[2]
#
# NOTE: The @view macro is implemented in Rust (lowering/stmt/macros.rs)
# because it requires AST inspection to transform A[i:j] to view(A, i:j).
# This definition is a placeholder for documentation purposes only.

# =============================================================================
# @views - Apply view semantics to all indexing in a block
# =============================================================================
# Based on Julia's base/views.jl
#
# @views transforms all A[...] indexing expressions within the block
# into view(A, ...) calls, except for:
# - Left-hand side of assignments (A[i] = x keeps its meaning)
# - Scalar indexing (A[i] where i is a single integer)
#
# Usage:
#   @views begin
#       x = arr[1:3]       # x is a view
#       y = arr[4:6]       # y is a view
#       arr[1:3] = arr[4:6]  # assignment - copies data
#   end
#
# NOTE: The @views macro is implemented in Rust for AST transformation.
# This definition is a placeholder for documentation purposes only.
