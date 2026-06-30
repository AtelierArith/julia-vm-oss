# Issue #5363: a user-defined struct that declares a built-in abstract supertype
# (`struct S <: Real`) was not recognized by method dispatch — `f(x::Real)`
# called with an `S` value raised NoMethodFound. The dispatch struct-parent
# fallback only handled user-defined abstract types (`AbstractUser`), not the
# built-in abstract numeric types (`Real`/`Number`/`Integer`/...), and did not
# follow the built-in abstract hierarchy for transitivity.

using Test

struct MyReal <: Real
    x::Float64
end

struct MyInt <: Integer
    n::Int64
end

struct Plain
    v::Int64
end

f(x::Real) = "real"
g(x::Number) = "number"
h(x::Integer) = "integer"
k(x::Real) = "k-real"
k(x::Any) = "k-any"

@testset "user struct subtypes built-in abstract in dispatch (#5363)" begin
    m = MyReal(3.0)
    @test f(m) == "real"
    @test g(m) == "number"           # transitivity: Real <: Number

    mi = MyInt(5)
    @test h(mi) == "integer"
    @test f(mi) == "real"            # transitivity: Integer <: Real
    @test g(mi) == "number"          # transitivity: Integer <: Real <: Number

    # A struct that is NOT a subtype must not match ::Real (no over-matching).
    @test k(Plain(1)) == "k-any"
end

true  # Test passed
