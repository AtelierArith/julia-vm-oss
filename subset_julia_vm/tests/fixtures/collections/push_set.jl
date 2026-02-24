# Test push!(Set, x) works for Set type (Issue #1828)
# push! on Set should add element and return modified Set

using Test

@testset "push! on Set" begin
    # push! adds element to Set
    s = Set([1, 2, 3])
    result = push!(s, 4)
    @test length(result) == 4
    @test 4 in result

    # push! on copy of Set
    s2 = Set([10, 20])
    cs = copy(s2)
    result2 = push!(cs, 30)
    @test length(result2) == 3
    @test 30 in result2

    # push! existing element (no-op for Set)
    s3 = Set([1, 2, 3])
    result3 = push!(s3, 2)
    @test length(result3) == 3

    # push! to empty Set
    s4 = Set{Int64}()
    result4 = push!(s4, 42)
    @test length(result4) == 1
    @test 42 in result4
end

true
