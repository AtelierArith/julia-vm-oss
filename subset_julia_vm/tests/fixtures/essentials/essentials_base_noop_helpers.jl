using Test

# Regression coverage for the Pure Julia base/ "silent-stub" audit
# (Issue #4703, follow-up to PR #4704). The helpers exercised here all
# have trivial one-line bodies that LOOK like unfinished stubs but are in
# fact the correct generic-dispatch fallbacks / identities, matching
# upstream julia/base. They are marked `# INTENTIONAL_NOOP (Issue #4703)`
# in the source and tracked by scripts/check_julia_base_stubs.sh.
#
# These assertions pin the observable behavior so the helpers cannot be
# silently deleted (as dead code) or silently changed to a wrong value
# without a test failure. Verified to match Julia 1.12 before committing.

@testset "identity passthrough (number.jl)" begin
    @test identity(42) == 42
    @test identity(3.0) === 3.0
    @test identity("x") == "x"
    @test identity(nothing) === nothing
end

@testset "real/conj/isreal real fallbacks (number.jl)" begin
    # real(x::Real) = x, conj(x::Real) = x, isreal(x::Real) = true
    @test real(7) == 7
    @test conj(7) == 7
    @test isreal(7) == true
    @test isreal(3.5) == true
end

@testset "isnothing / ismissing identity comparisons" begin
    @test isnothing(nothing) == true
    @test isnothing(0) == false
    @test isnothing("") == false
    @test ismissing(missing) == true
    @test ismissing(0) == false
    @test ismissing(nothing) == false
end

@testset "firstindex of 1-based containers (range.jl)" begin
    @test firstindex([10, 20, 30]) == 1
    @test firstindex([1.0, 2.0]) == 1
end

true
