using Test

# Missing method: clamp(x::Integer, r::AbstractUnitRange) constrains an integer to
# a unit range's bounds (clamp(x, first(r), last(r))). Previously NoMethodFound.

@testset "clamp(x::Integer, r::AbstractUnitRange)" begin
    @test clamp(5, 1:10) == 5
    @test clamp(0, 1:10) == 1
    @test clamp(15, 1:10) == 10
    @test clamp(1, 1:10) == 1
    @test clamp(10, 1:10) == 10
    @test clamp(-3, 0:100) == 0
    @test clamp(5, 1:10) isa Int

    # 3-argument clamp is unchanged.
    @test clamp(5, 1, 10) == 5
    @test clamp(3.5, 1.0, 5.0) == 3.5
    @test clamp(-1, 0, 9) == 0
end

true
