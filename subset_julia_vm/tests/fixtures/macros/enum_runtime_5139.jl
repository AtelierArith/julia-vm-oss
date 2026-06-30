# @enum runtime integration (Issue #5139)
#
# Verifies that `@enum` members bind at runtime, the enum type name resolves to
# a DataType, members convert to integers, the type is callable for value->member
# construction, `instances` returns the declaration-ordered tuple, and display
# shows the member name (not `Type(value)`). All expectations match upstream
# Julia 1.12.

using Test

@enum Color red green blue
@enum Fruit apple=1 orange=2 kiwi=3

@testset "enum members bind at runtime" begin
    # Bare member references resolve to enum values, not UndefVarError.
    @test red === red
    @test red !== green
    @test Color(2) === blue
end

@testset "enum -> integer conversion" begin
    @test Int(red) == 0
    @test Int(green) == 1
    @test Int(blue) == 2
    @test Int(kiwi) == 3
    @test Int(apple) == 1
end

@testset "enum construction from integer" begin
    @test Color(0) === red
    @test Color(2) === blue
    @test Fruit(1) === apple
    @test Fruit(3) === kiwi
end

@testset "enum type name resolves to a type" begin
    # `Color` must resolve (no UndefVarError) and be the type of its members.
    @test typeof(red) === Color
    @test typeof(kiwi) === Fruit
end

@testset "instances returns declaration-ordered tuple" begin
    @test instances(Color) === (red, green, blue)
    @test instances(Fruit) === (apple, orange, kiwi)
end

@testset "enum display shows member name" begin
    # `string` / `print` / `println` render the member name, matching upstream
    # (the issue's `red` REPL-display probe). `repr`/`show`-dispatch on the
    # abstract `Enum` supertype is a separate lattice concern, tracked apart from
    # this issue's listed behaviors.
    @test string(red) == "red"
    @test string(green) == "green"
    @test string(blue) == "blue"
    @test string(kiwi) == "kiwi"

    buf = IOBuffer()
    print(buf, red)
    @test String(take!(buf)) == "red"
end

true
