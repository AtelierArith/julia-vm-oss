# Range stored in variable should iterate with Int64, not Float64
# Issue #2106

using Test

@testset "Range variable iteration produces Int64" begin
    # Inline range - always worked
    for i in 1:3
        @test typeof(i) == Int64
    end

    # Stored range - was broken (produced Float64)
    r = 1:3
    for i in r
        @test typeof(i) == Int64
    end

    # eachindex returns a range - was broken when used for indexing
    a = [10, 20, 30]
    for i in eachindex(a)
        @test typeof(i) == Int64
        @test a[i] == a[Int64(i)]
    end

    # eachindex with actual indexing
    a = [10, 20, 30]
    results = Int64[]
    for i in eachindex(a)
        push!(results, a[i])
    end
    @test results == [10, 20, 30]

    # Range from variable endpoint
    n = 5
    r2 = 1:n
    vals = Int64[]
    for i in r2
        push!(vals, i)
    end
    @test vals == [1, 2, 3, 4, 5]

    # Float range should still produce Float64
    r3 = 1.0:0.5:3.0
    for v in r3
        @test typeof(v) == Float64
    end
end

true
