using Test

@testset "HOF predicate reducer specialization (Issue #5094)" begin
    xs = Int32[-3, 0, 1, 4, 5]

    @test any(iszero, xs)
    @test any(isone, xs)
    @test any(signbit, xs)
    @test any(iseven, xs)
    @test any(isodd, xs)

    @test !all(iszero, xs)
    @test !all(isone, xs)
    @test !all(signbit, xs)
    @test !all(iseven, xs)
    @test !all(isodd, xs)

    @test all(iszero, Int32[0, 0])
    @test all(isone, Int32[1, 1])
    @test all(signbit, Int32[-3, -1])
    @test all(iseven, Int32[0, 4])
    @test all(isodd, Int32[-3, 1])

    @test count(iszero, xs) == 1
    @test count(isone, xs) == 1
    @test count(signbit, xs) == 1
    @test count(iseven, xs) == 2
    @test count(isodd, xs) == 3

    @test findall(iszero, xs) == [2]
    @test findall(isone, xs) == [3]
    @test findall(signbit, xs) == [1]
    @test findall(iseven, xs) == [2, 4]
    @test findall(isodd, xs) == [1, 3, 5]
    @test typeof(findall(isodd, xs)) == Vector{Int64}

    empty = Int32[]
    @test !any(iszero, empty)
    @test all(iszero, empty)
    @test count(iszero, empty) == 0
    @test isempty(findall(iszero, empty))
end

true
