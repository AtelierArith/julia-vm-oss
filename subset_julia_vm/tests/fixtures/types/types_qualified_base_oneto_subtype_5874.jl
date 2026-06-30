# Issue #5874: a qualified Base TYPE (e.g. `Base.OneTo`) used in a subtype
# expression must resolve to the type object, not be looked up as a Base function.
# Previously `Base.OneTo <: AbstractUnitRange` failed to compile with
# "Base has no function named OneTo".

using Test

@testset "qualified Base.OneTo in subtype expression (Issue #5874)" begin
    @test (Base.OneTo <: AbstractUnitRange) == true
    @test (Base.OneTo <: AbstractRange) == true

    # the qualified type object is a Type.
    @test Base.OneTo isa Type

    # other qualified Base type objects keep working in subtype position.
    @test (Base.RefValue <: Ref) == true
end

true
