# Phase 4-3: Set core operations via Pure Julia intrinsics (Issue #2574)
# Tests that push!, delete!, in, empty!, length work through Pure Julia wrappers

using Test

@testset "Set core operations via Pure Julia" begin
    # push! - add element to set
    s = Set()
    push!(s, 1)
    push!(s, 2)
    push!(s, 3)
    @test length(s) == 3
    @test 1 in s
    @test 2 in s
    @test 3 in s

    # push! duplicate - no effect on length
    push!(s, 2)
    @test length(s) == 3

    # in - membership test
    @test 1 in s
    @test !(4 in s)

    # delete! - remove element from set
    delete!(s, 2)
    @test length(s) == 2
    @test !(2 in s)
    @test 1 in s
    @test 3 in s

    # delete! non-existent element - no error
    delete!(s, 99)
    @test length(s) == 2

    # empty! - clear all elements
    empty!(s)
    @test length(s) == 0
    @test !(1 in s)

    # length - empty set
    s2 = Set()
    @test length(s2) == 0
    push!(s2, 10)
    push!(s2, 20)
    @test length(s2) == 2
end

true
