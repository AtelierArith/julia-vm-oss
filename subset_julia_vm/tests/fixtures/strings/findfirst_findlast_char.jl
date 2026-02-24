# findfirst/findlast with Char and String patterns (Issue #2030)
# Char pattern returns Int64 index, String pattern returns UnitRange{Int64}

using Test

@testset "findfirst/findlast char and string patterns (Issue #2030)" begin
    # findfirst with Char pattern - returns Int64
    @test findfirst('l', "hello") == 3
    @test findfirst('h', "hello") == 1
    @test findfirst('o', "hello") == 5
    @test findfirst('x', "hello") === nothing

    # findlast with Char pattern - returns Int64
    @test findlast('l', "hello") == 4
    @test findlast('h', "hello") == 1
    @test findlast('x', "hello") === nothing

    # findfirst with String pattern - returns UnitRange
    r1 = findfirst("ll", "hello")
    @test first(r1) == 3
    @test last(r1) == 4

    r2 = findfirst("he", "hello")
    @test first(r2) == 1
    @test last(r2) == 2

    @test findfirst("xx", "hello") === nothing

    # findlast with String pattern - returns UnitRange
    r3 = findlast("ll", "hello llama")
    @test first(r3) == 7
    @test last(r3) == 8

    r4 = findlast("he", "hello")
    @test first(r4) == 1
    @test last(r4) == 2

    @test findlast("xx", "hello") === nothing

    # Single character String pattern - UnitRange with same start/end
    r5 = findfirst("l", "hello")
    @test first(r5) == 3
    @test last(r5) == 3

    # Predicate form still works
    @test findfirst(x -> x > 3, [1, 2, 5, 4]) == 3
end

true
