# =============================================================================
# timing.jl - Timing and profiling macros
# =============================================================================
# Based on Julia's base/timing.jl

# =============================================================================
# @time - Measure and print execution time
# =============================================================================
# This is a simplified version of Julia's @time macro.
# It measures wall-clock time using time_ns() and prints the elapsed time.
#
# Note: We use $ex directly instead of $(esc(ex)) because our hygiene system
# already renames local variables (t0, result, etc.) to avoid collisions.
# The expression $ex is substituted with the caller's expression at macro
# expansion time.
macro time(ex)
    quote
        local t0 = time_ns()
        local result = $ex
        local elapsed_ns = time_ns() - t0
        local elapsed_s = elapsed_ns / 1.0e9
        println("  ", elapsed_s, " seconds")
        result
    end
end

# =============================================================================
# @elapsed - Measure execution time and return seconds
# =============================================================================
# This is a simplified version of Julia's @elapsed macro.
# It measures wall-clock time using time_ns() and returns the elapsed time
# in seconds as a Float64. Unlike @time, it does not print anything.
#
# Usage:
#   elapsed = @elapsed sleep(0.1)  # Returns ~0.1
macro elapsed(ex)
    quote
        local t0 = time_ns()
        $(esc(ex))
        (time_ns() - t0) / 1.0e9
    end
end

# =============================================================================
# @timed - Measure execution time and return NamedTuple
# =============================================================================
# This is a simplified version of Julia's @timed macro.
# Returns a NamedTuple with fields:
#   - value: the result of the expression
#   - time: elapsed time in seconds
#
# The full Julia @timed returns additional fields (bytes, gctime, gcstats,
# lock_conflicts, compile_time, recompile_time), but SubsetJuliaVM doesn't
# have GC/compilation tracking, so we only return the essential fields.
#
# Usage:
#   t = @timed compute_something()
#   t.value  # the result
#   t.time   # elapsed seconds
#
# For tuple destructuring (Julia compatibility):
#   result, elapsed = @timed compute_something()
macro timed(ex)
    quote
        local t0 = time_ns()
        local result = $(esc(ex))
        local elapsed_s = (time_ns() - t0) / 1.0e9
        (value=result, time=elapsed_s)
    end
end

# =============================================================================
# @timev - Verbose time measurement
# =============================================================================
# This is a simplified version of Julia's @timev macro.
# It measures wall-clock time and prints verbose output including:
#   - elapsed time in seconds
#   - elapsed time in nanoseconds
#
# Note: SubsetJuliaVM doesn't have GC or compilation tracking, so we only
# report timing information. Full Julia @timev also shows:
#   - gc time, bytes allocated, pool allocs, GC pauses, etc.
#
# Usage:
#   @timev sum(1:1000)
#   @timev "Computing sum" sum(1:1000)  # with optional message
macro timev(ex)
    quote
        local t0 = time_ns()
        local result = $(esc(ex))
        local elapsed_ns = time_ns() - t0
        local elapsed_s = elapsed_ns / 1.0e9
        println("  ", elapsed_s, " seconds")
        println("elapsed time (ns):  ", elapsed_ns)
        result
    end
end

macro timev(msg, ex)
    quote
        local msg_val = $(esc(msg))
        local t0 = time_ns()
        local result = $(esc(ex))
        local elapsed_ns = time_ns() - t0
        local elapsed_s = elapsed_ns / 1.0e9
        println(msg_val)
        println("  ", elapsed_s, " seconds")
        println("elapsed time (ns):  ", elapsed_ns)
        result
    end
end

# =============================================================================
# @showtime - Show expression and measure execution time
# =============================================================================
# This is a simplified version of Julia's @showtime macro.
# Like @time but also prints the expression being evaluated for reference.
#
# Usage:
#   @showtime sum(1:1000)
#   # Output: sum(1:1000): 0.001234 seconds
#
# Note: @showtime was added in Julia 1.8.
macro showtime(ex)
    # Get expression as string to print before timing output
    expr_str = string(ex)
    quote
        local t0 = time_ns()
        local result = $(esc(ex))
        local elapsed_ns = time_ns() - t0
        local elapsed_s = elapsed_ns / 1.0e9
        println($expr_str, ": ", elapsed_s, " seconds")
        result
    end
end

# =============================================================================
# @allocated - Return bytes allocated (stub implementation)
# =============================================================================
# This is a stub implementation of Julia's @allocated macro.
# SubsetJuliaVM doesn't have GC integration, so this always returns 0.
#
# In full Julia, @allocated measures heap allocations during expression
# evaluation. Since SubsetJuliaVM is an AOT-compiled VM without a traditional
# garbage collector, we cannot measure allocations.
#
# Usage:
#   bytes = @allocated sum(1:1000)
#   # Returns 0 (stub implementation)
#
# Note: This stub allows code using @allocated to run without modification,
# but the returned value is not meaningful.
macro allocated(ex)
    quote
        $(esc(ex))
        0  # Stub: always return 0 bytes
    end
end

# =============================================================================
# @allocations - Return allocation count (stub implementation)
# =============================================================================
# This is a stub implementation of Julia's @allocations macro.
# SubsetJuliaVM doesn't have GC integration, so this always returns 0.
#
# In full Julia, @allocations counts the number of heap allocations during
# expression evaluation. Since SubsetJuliaVM doesn't track allocations,
# we return 0.
#
# Usage:
#   count = @allocations rand(1000)
#   # Returns 0 (stub implementation)
#
# Note: This stub allows code using @allocations to run without modification,
# but the returned value is not meaningful.
macro allocations(ex)
    quote
        $(esc(ex))
        0  # Stub: always return 0 allocations
    end
end
