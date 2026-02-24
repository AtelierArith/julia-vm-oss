# Meta.lower: Lower expressions to Core IR representation
#
# Meta.lower(m, x) takes an expression and returns the lowered form.
# - For literals, returns them unchanged
# - For symbols, returns them unchanged
# - For expressions, performs lowering (desugaring) to Core IR

using Test

@testset "Meta.lower with literals" begin
    # Lowering integers returns them unchanged
    r1 = Meta.lower(Main, 42)
    @test (r1 == 42)

    # Lowering booleans returns them unchanged
    r3 = Meta.lower(Main, true)
    @test (r3 == true)

    # Lowering nothing returns nothing
    r6 = Meta.lower(Main, nothing)
    @test (r6 === nothing)
end

@testset "Meta.lower with symbols" begin
    # Lowering symbols returns them unchanged
    r1 = Meta.lower(Main, :x)
    @test (r1 == :x)

    r2 = Meta.lower(Main, :foo)
    @test (r2 == :foo)
end

@testset "Meta.lower single argument form" begin
    # The single-argument form should also work
    r1 = Meta.lower(42)
    @test (r1 == 42)

    r2 = Meta.lower(:x)
    @test (r2 == :x)
end

true
