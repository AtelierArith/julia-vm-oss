using Test

# Prevention test: SplatParameter/SplatExpression duality (Issue #2253)
# Verifies that both full-form and short-form function definitions produce
# identical behavior for all varargs patterns.
#
# Full-form: `function f(args...) ... end` → SplatParameter
# Short-form: `f(args...) = expr` → SplatExpression

# =============================================================================
# Pattern 1: Basic positional varargs
# =============================================================================

# Full-form
function sum_full(args...)
    sum(args)
end

# Short-form
sum_short(args...) = sum(args)

# =============================================================================
# Pattern 2: Mixed positional + varargs
# =============================================================================

# Full-form
function mixed_full(x, rest...)
    x + sum(rest)
end

# Short-form
mixed_short(x, rest...) = x + sum(rest)

# =============================================================================
# Pattern 3: Kwargs varargs
# =============================================================================

# Full-form
function kwargs_full(; kwargs...)
    length(kwargs)
end

# Short-form
kwargs_short(; kwargs...) = length(kwargs)

# =============================================================================
# Pattern 4: Mixed positional + kwargs varargs
# =============================================================================

# Full-form
function pos_and_kwargs_full(x; kwargs...)
    x + length(kwargs)
end

# Short-form
pos_and_kwargs_short(x; kwargs...) = x + length(kwargs)

# =============================================================================
# Tests
# =============================================================================

# Pre-compute results outside @testset
r_sum_full = sum_full(1, 2, 3)
r_sum_short = sum_short(1, 2, 3)
r_mixed_full = mixed_full(10, 1, 2, 3)
r_mixed_short = mixed_short(10, 1, 2, 3)
r_kwargs_full = kwargs_full(a=1, b=2, c=3)
r_kwargs_short = kwargs_short(a=1, b=2, c=3)
r_pos_kw_full = pos_and_kwargs_full(100, x=1, y=2)
r_pos_kw_short = pos_and_kwargs_short(100, x=1, y=2)

@testset "SplatParameter/SplatExpression duality (Issue #2253)" begin
    @testset "Pattern 1: Basic positional varargs" begin
        @test r_sum_full == r_sum_short
        @test r_sum_full == 6
    end

    @testset "Pattern 2: Mixed positional + varargs" begin
        @test r_mixed_full == r_mixed_short
        @test r_mixed_full == 16
    end

    @testset "Pattern 3: Kwargs varargs" begin
        @test r_kwargs_full == r_kwargs_short
        @test r_kwargs_full == 3
    end

    @testset "Pattern 4: Mixed positional + kwargs varargs" begin
        @test r_pos_kw_full == r_pos_kw_short
        @test r_pos_kw_full == 102
    end
end

true
