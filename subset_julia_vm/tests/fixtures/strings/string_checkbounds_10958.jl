# checkbounds(::String, ::UnitRange) (Issue #10958)

using Test

@testset "string range checkbounds" begin
    @test checkbounds("abc", 2:3) === nothing
    @test checkbounds("abc", 1:3) === nothing
    @test checkbounds("abc", 2:1) === nothing  # empty range is in bounds
    @test_throws BoundsError checkbounds("abc", 2:4)
    @test_throws BoundsError checkbounds("abc", 0:2)
    @test checkbounds(Bool, "abc", 2:3)
    @test !checkbounds(Bool, "abc", 2:4)
    @test checkbounds(Bool, "abc", 3)
    @test !checkbounds(Bool, "abc", 4)
    @test checkbounds("abc", 2) === nothing
    @test_throws BoundsError checkbounds("abc", 5)
end

true
