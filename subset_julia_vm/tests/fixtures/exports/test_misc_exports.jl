# Test exported misc functions (getindex, sprint, uppercase, lowercase, parse, error)

using Test

@testset "Misc exported functions" begin
    # getindex - array indexing
    arr = [10, 20, 30]
    @test getindex(arr, 1) == 10
    @test getindex(arr, 2) == 20
    @test getindex(arr, 3) == 30

    # sprint - string from print
    s = sprint(print, "hello")
    @test s === "hello"

    # uppercase/lowercase
    @test uppercase("hello") === "HELLO"
    @test lowercase("HELLO") === "hello"
    @test uppercase("abc123") === "ABC123"
    @test lowercase("ABC123") === "abc123"

    # parse - string to number
    @test parse(Int64, "42") === 42
    @test parse(Float64, "3.14") === 3.14
end

true
