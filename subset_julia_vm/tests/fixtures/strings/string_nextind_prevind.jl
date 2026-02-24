# Test nextind and prevind functions - UTF-8 string index navigation

using Test

@testset "nextind/prevind - UTF-8 index navigation" begin

    # === nextind with ASCII strings ===
    s = "hello"
    @assert nextind(s, 0) == 1
    @assert nextind(s, 1) == 2
    @assert nextind(s, 2) == 3
    @assert nextind(s, 4) == 5
    @assert nextind(s, 5) == 6

    # === prevind with ASCII strings ===
    @assert prevind(s, 1) == 0
    @assert prevind(s, 2) == 1
    @assert prevind(s, 3) == 2
    @assert prevind(s, 5) == 4

    # === Edge cases ===
    @assert prevind(s, 0) == 0
    @assert nextind(s, 6) == 6

    # === Empty string ===
    empty = ""
    @assert nextind(empty, 0) == 1
    @assert prevind(empty, 1) == 0

    # All tests passed
    @test (true)
end

true  # Test passed
