# Test short-circuit && and || with throw()/error() (Issue #2598)
# In Julia, throw() returns Union{} (bottom type) which is compatible
# with any type context. SubsetJuliaVM must not require Bool conversion.
using Test

function check_positive(x::Int64)
    x <= 0 && throw(ArgumentError("x must be positive"))
    return x
end

function check_nonempty(s::String)
    isempty(s) && error("string must be non-empty")
    return s
end

function require_positive(x::Int64)
    x > 0 || throw(ArgumentError("x must be positive"))
    return x
end

@testset "&& with throw()" begin
    # Normal case: condition is false, throw is NOT executed
    @test check_positive(5) == 5
    @test check_positive(1) == 1

    # Error case: condition is true, throw IS executed
    @test_throws ArgumentError check_positive(0)
    @test_throws ArgumentError check_positive(-1)
end

@testset "&& with error()" begin
    @test check_nonempty("hello") == "hello"
    @test_throws ErrorException check_nonempty("")
end

@testset "|| with throw()" begin
    # x > 0 is true → short-circuit, no throw
    @test require_positive(5) == 5
    @test require_positive(1) == 1

    # x > 0 is false → evaluate throw
    @test_throws ArgumentError require_positive(0)
    @test_throws ArgumentError require_positive(-1)
end

true
