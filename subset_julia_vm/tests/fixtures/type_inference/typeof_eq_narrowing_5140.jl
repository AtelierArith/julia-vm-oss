using Test

# Issue #5140: Conditional lattice narrowing for `typeof(x) == T` / `typeof(x) === T`.
#
# `x isa T` and `x === nothing` already narrow `x` in their branches via
# environment splitting. `typeof(x) === T` (and `==`, and `!==`/`!=`, and the
# reversed operand order) did NOT narrow, so a `Union{Int,String}` argument
# stayed widened (the ternary inferred `Any`).
#
# For a concrete `T`, `typeof(x) === T` is equivalent to `x isa T` for narrowing:
# the then-branch refines `x` to `T`, the else-branch subtracts `T`. The result
# type of the function should match upstream Julia exactly.

typeof_egal_narrow_5140(x::Union{Int,String}) =
    typeof(x) === Int ? x + 1 : length(x)

typeof_eq_narrow_5140(x::Union{Int,String}) =
    typeof(x) == Int ? x + 1 : length(x)

typeof_reversed_narrow_5140(x::Union{Int,String}) =
    Int === typeof(x) ? x + 1 : length(x)

typeof_notegal_narrow_5140(x::Union{Int,String}) =
    typeof(x) !== Int ? length(x) : x + 1

# Different-typed branches: then `x` is Int (x+1 -> Int), else `x` is Float64
# (x*2.0 -> Float64). Result is the join Union{Int64,Float64}.
typeof_mixed_branch_5140(x::Union{Int,Float64}) =
    typeof(x) === Int ? x + 1 : x * 2.0

# Regression guard: `x === nothing` narrowing must still work after adding the
# typeof arms ahead of the nothing-check arms.
nothing_still_narrows_5140(x::Union{Int,Nothing}) =
    x === nothing ? 0 : x + 1

@testset "typeof(x) === T / == T narrowing (Issue #5140)" begin
    # Inferred return types match upstream Julia.
    @test Base.infer_return_type(typeof_egal_narrow_5140, Tuple{Union{Int,String}}) === Int64
    @test Base.infer_return_type(typeof_eq_narrow_5140, Tuple{Union{Int,String}}) === Int64
    @test Base.infer_return_type(typeof_reversed_narrow_5140, Tuple{Union{Int,String}}) === Int64
    @test Base.infer_return_type(typeof_notegal_narrow_5140, Tuple{Union{Int,String}}) === Int64
    @test Base.infer_return_type(typeof_mixed_branch_5140, Tuple{Union{Int,Float64}}) ===
          Union{Int64,Float64}
    @test Base.infer_return_type(nothing_still_narrows_5140, Tuple{Union{Int,Nothing}}) === Int64

    # Runtime results are branch-correct.
    @test typeof_egal_narrow_5140(3) == 4
    @test typeof_egal_narrow_5140("abc") == 3
    @test typeof_notegal_narrow_5140(3) == 4
    @test typeof_notegal_narrow_5140("abc") == 3
    @test typeof_mixed_branch_5140(3) == 4
    @test typeof_mixed_branch_5140(2.0) == 4.0
    @test nothing_still_narrows_5140(nothing) == 0
    @test nothing_still_narrows_5140(5) == 6
end

true
