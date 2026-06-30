# `==` / `isequal` element-compare against an AbstractArray-subtype operand that
# is neither a native array nor a StaticArrays carrier — a user
# `struct <: AbstractVector` and a `SubArray` view (Issue #8229). Previously
# these returned object-identity `false` because the equality builtin could not
# read the operand, and the operand also did not match `::AbstractArray` method
# parameters (so the Pure-Julia element-wise `isequal` was unreachable).
using Test

struct MyVec <: AbstractVector{Float64}
    data::Vector{Float64}
end
Base.size(v::MyVec) = size(v.data)
Base.getindex(v::MyVec, i::Int) = v.data[i]

@testset "user struct <: AbstractVector equality" begin
    v = MyVec([1.0, 2.0, 3.0])

    # Element-wise, not object identity.
    @test isequal(v, [1.0, 2.0, 3.0])
    @test v == [1.0, 2.0, 3.0]
    @test [1.0, 2.0, 3.0] == v
    @test v == MyVec([1.0, 2.0, 3.0])
    @test isequal(v, MyVec([1.0, 2.0, 3.0]))

    # Distinct contents compare unequal.
    @test !(v == MyVec([1.0, 2.0, 9.0]))
    @test v != MyVec([1.0, 2.0, 9.0])
    @test v != [1.0, 2.0, 9.0]
    @test !isequal(v, [1.0, 2.0, 9.0])

    # Shape mismatch is unequal, not an error.
    @test v != [1.0, 2.0]
    @test !isequal(v, [1.0, 2.0, 3.0, 4.0])
end

@testset "SubArray view equality" begin
    w = view([1, 2, 3, 4], 1:3)
    @test isequal(w, [1, 2, 3])
    @test w == [1, 2, 3]
    @test [1, 2, 3] == w
    @test w != [1, 2, 9]
    @test !(w == [1, 2, 9])
end

@testset "AbstractArray-subtype struct dispatches to ::AbstractArray methods" begin
    # The struct's declared supertype reaches AbstractArray only through the
    # built-in grandparent link AbstractVector{T} <: AbstractArray; static
    # dispatch must resolve it instead of raising a MethodError.
    onlyarray(x::AbstractArray) = "abstractarray"
    v = MyVec([1.0, 2.0, 3.0])
    w = view([1, 2, 3, 4], 1:3)
    @test onlyarray(v) == "abstractarray"
    @test onlyarray(w) == "abstractarray"
    @test v isa AbstractArray
    @test w isa AbstractArray
end

true
