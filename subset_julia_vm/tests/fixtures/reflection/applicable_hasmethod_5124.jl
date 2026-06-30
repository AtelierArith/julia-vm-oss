# applicable(f, args...) / hasmethod(f, Tuple{...}) method-existence checks (Issue #5124)
#
# Both query whether a dispatchable method exists for a function WITHOUT
# executing its body. `applicable` is value-based (uses the runtime types
# of the supplied arguments); `hasmethod` is type-based (uses an explicit
# Tuple{...} signature). Mirrors upstream Julia's `applicable`
# (julia/src/builtins.c jl_f_applicable) and `hasmethod`
# (julia/base/reflection.jl). Verified against upstream Julia 1.12.6.
using Test

# Single-method user function over a concrete type.
f5124(x::Int) = x + 1
# Two-argument user function.
g5124(x::Int, y::Int) = x + y
# Method over an abstract type, so a concrete subtype must also match.
h5124(x::Number) = x
# Vararg method.
v5124(xs::Int...) = length(xs)

abstract type Shape5124 end
struct Circle5124 <: Shape5124 end
struct Square5124 <: Shape5124 end

area5124(::Circle5124) = 1
# Note: deliberately NO method for Square5124 so dispatch must fail.

@testset "applicable: value-based method existence (Issue #5124)" begin
    # Defined method -> true.
    @test applicable(f5124, 1) == true
    @test applicable(g5124, 1, 2) == true
    @test applicable(area5124, Circle5124()) == true

    # Abstract-typed method matches a concrete subtype value.
    @test applicable(h5124, 1) == true
    @test applicable(h5124, 1.5) == true

    # Argument-type mismatch -> false.
    @test applicable(f5124, "a") == false
    @test applicable(g5124, 1, "a") == false
    @test applicable(h5124, "a") == false
    @test applicable(area5124, Square5124()) == false

    # Wrong arity -> false.
    @test applicable(f5124) == false
    @test applicable(f5124, 1, 2) == false
    @test applicable(g5124, 1) == false

    # Vararg: any arity of the right element type is applicable.
    @test applicable(v5124) == true
    @test applicable(v5124, 1, 2, 3) == true
    @test applicable(v5124, 1, "x") == false

    # Built-in operator dispatch.
    @test applicable(+, 1, 2) == true
    @test applicable(+, 1, "a") == false

    # Result is a Bool.
    @test applicable(f5124, 1) isa Bool
end

@testset "hasmethod: type-based method existence (Issue #5124)" begin
    # Defined signature -> true.
    @test hasmethod(f5124, Tuple{Int}) == true
    @test hasmethod(g5124, Tuple{Int, Int}) == true
    @test hasmethod(area5124, Tuple{Circle5124}) == true

    # Abstract signature and concrete subtype signature both match.
    @test hasmethod(h5124, Tuple{Number}) == true
    @test hasmethod(h5124, Tuple{Int}) == true

    # Argument-type mismatch -> false.
    @test hasmethod(f5124, Tuple{String}) == false
    @test hasmethod(g5124, Tuple{Int, String}) == false
    @test hasmethod(h5124, Tuple{String}) == false
    @test hasmethod(area5124, Tuple{Square5124}) == false

    # Wrong arity -> false.
    @test hasmethod(f5124, Tuple{}) == false
    @test hasmethod(f5124, Tuple{Int, Int}) == false
    @test hasmethod(g5124, Tuple{Int}) == false

    # Result is a Bool.
    @test hasmethod(f5124, Tuple{Int}) isa Bool
end

@testset "applicable / hasmethod agree (Issue #5124)" begin
    # applicable(f, args...) is equivalent to hasmethod(f, typeof(args)).
    @test applicable(f5124, 1) == hasmethod(f5124, Tuple{Int})
    @test applicable(f5124, "a") == hasmethod(f5124, Tuple{String})
    @test applicable(g5124, 1, 2) == hasmethod(g5124, Tuple{Int, Int})
    @test applicable(h5124, 1.5) == hasmethod(h5124, Tuple{Float64})
end

true
