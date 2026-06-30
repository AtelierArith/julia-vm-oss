# Parametric struct construction with abstract type-parameter bounds (Issue #5060)
#
# When a parametric struct field is bound to an *abstract* numeric type
# (Real, Number, AbstractFloat) the default constructor must NOT force-coerce
# the argument to a concrete representation. Julia keeps the original concrete
# value (`Foo{Real}(1).x` is the `Int64` 1, not the `Float64` 1.0), inserting
# only `convert(fieldtype, x)` which is a no-op when `x isa fieldtype`.
#
# Concrete bounds (Foo{Int}, Foo{Float64}) still convert via convert(fieldtype, x).

using Test

struct Foo{T}
    x::T
end

struct Holder
    n::Number
    r::Real
    i::Integer
end

@testset "parametric abstract field bound preserves concrete type" begin
    # Abstract parameter bound: value preserved as-is.
    @test typeof(Foo{Real}(1).x) === Int64
    @test Foo{Real}(1).x === 1
    @test typeof(Foo{Number}(1).x) === Int64
    @test typeof(Foo{Real}(1.5).x) === Float64
    @test Foo{Real}(1.5).x === 1.5

    # Concrete parameter bound: convert(fieldtype, x) still applies.
    @test typeof(Foo{Int}(1.0).x) === Int64
    @test Foo{Int}(1.0).x === 1
    @test typeof(Foo{Float64}(1).x) === Float64
    @test Foo{Float64}(1).x === 1.0
end

@testset "abstract-typed non-parametric field preserves runtime concrete type" begin
    h = Holder(1, 2, 3)
    @test typeof(h.n) === Int64
    @test typeof(h.r) === Int64
    @test typeof(h.i) === Int64

    h2 = Holder(1.0, 2.5, 3)
    @test typeof(h2.n) === Float64
    @test typeof(h2.r) === Float64
    @test typeof(h2.i) === Int64
end

true  # Test passed
