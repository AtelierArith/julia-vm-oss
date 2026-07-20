# Constructor return identity must not depend on sibling registration order.

using Test

module Forward11436
struct Inner{T}
    x::T
end
struct Outer{T}
    x::T
end

classify(x::Outer{Inner{Number}}) = "bad"
classify(x) = "ok"
classify_any(x::Any) = classify(x)
classify_dynamic(x::Outer{Int64}) = "int"
classify_dynamic(x::Outer{Float64}) = "float"
construct_dynamic(x) = classify_dynamic(Outer(x))

seed() = (Outer(Inner(1.0)), Outer(Inner(true)))
target() = Outer(Inner(1))
function check()
    seed()
    value = target()
    return (
        typeof(value) == Outer{Inner{Int64}},
        classify(value),
        classify_any(value),
        construct_dynamic(1),
        construct_dynamic(1.0),
    )
end
end

module Reverse11436
struct Inner{T}
    x::T
end
struct Outer{T}
    x::T
end

classify(x) = "ok"
classify(x::Outer{Inner{Number}}) = "bad"
classify_any(x::Any) = classify(x)
classify_dynamic(x::Outer{Int64}) = "int"
classify_dynamic(x::Outer{Float64}) = "float"
construct_dynamic(x) = classify_dynamic(Outer(x))

seed() = (Outer(Inner(true)), Outer(Inner(1.0)))
target() = Outer(Inner(1))
function check()
    seed()
    value = target()
    return (
        typeof(value) == Outer{Inner{Int64}},
        classify(value),
        classify_any(value),
        construct_dynamic(1),
        construct_dynamic(1.0),
    )
end
end

@testset "constructor return exact-or-Any identity" begin
    # Keep the complete type expression inside its defining module. Applying
    # nested parametric types through a runtime module value is tracked by #11463.
    @test Forward11436.check() == (true, "ok", "ok", "int", "float")
    @test Reverse11436.check() == (true, "ok", "ok", "int", "float")
end

# Keep the added methods module-local. Unrelated later global methods affecting
# earlier qualified same-leaf calls are tracked separately by #11471.
module ComplexReturn11468
complex_kind(x::Complex{Int64}) = "int"
complex_kind(x::Complex{Float64}) = "float"
dynamic_complex_kind(x) = complex_kind(complex(x))
results() = (dynamic_complex_kind(1), dynamic_complex_kind(1.0))
end

@testset "unknown Complex constructor return identity" begin
    # Unknown `complex(x)` dispatches from the runtime `Complex{T}` identity,
    # never a hard-coded `Complex{Float64}` inference result (Issue #11468).
    @test ComplexReturn11468.results() == ("int", "float")

    # An exact runtime Complex value whose constructor expression remains
    # statically dynamic must still reach Pure Julia sqrt dispatch (#11481).
    z = 1.0 + 2.0im
    w = 1 - z
    @test sqrt(complex(real(w), -imag(w))) == 1.0 + 1.0im
end

module OwnerFirstA11469
struct BoxF
    x::Int64
    y::Int64
    BoxF(x::String) = new(length(x), 0)
end
construct(x) = BoxF(x)
end

module OwnerFirstB11469
export BoxF
struct BoxF
    x::Int64
end
end

using .OwnerFirstB11469

module SiblingFirstB11469
export BoxR
struct BoxR
    x::Int64
end
end

using .SiblingFirstB11469

module SiblingFirstA11469
struct BoxR
    x::Int64
    y::Int64
    BoxR(x::String) = new(length(x), 0)
end
construct(x) = BoxR(x)
end

module OwnerChecks11469
function raises_method_error(f)
    try
        f(1)
        false
    catch e
        e isa MethodError
    end
end
results() = (
    raises_method_error(Main.OwnerFirstA11469.construct),
    raises_method_error(Main.SiblingFirstA11469.construct),
)
end

@testset "default constructor fallback preserves owner" begin
    @test OwnerChecks11469.results() == (true, true)
end

module OwnerInferenceA11510
struct W{T}
    x::Vector{T}
end
kind(x::W{Int64}) = "int"
make(x::Vector{Int64}) = kind(W(x))
end

module OwnerInferenceB11510
struct W{T}
    x::T
end
end

@testset "parametric return inference uses the resolved owner" begin
    @test OwnerInferenceA11510.make([1]) == "int"
end

module TypedArrayA11511
struct W{T}
    x::T
end
function check()
    xs = W{Int64}[W{Int64}(1)]
    return eltype(xs) == W{Int64} && typeof(xs) == Vector{W{Int64}}
end
end

module TypedArrayB11511
struct W{T}
    x::T
end
end

@testset "typed-array element identity uses the lexical owner" begin
    @test TypedArrayA11511.check()
end

true
