using Test

@testset "Memory{T} with larger allocations and iteration" begin
    # Large Memory{Int64}: 100 elements
    n = 100
    m = Memory{Int64}(n)
    @test length(m) == n

    # Populate with 1..n
    for i in 1:n
        m[i] = i
    end
    @test m[1] == 1
    @test m[50] == 50
    @test m[100] == 100

    # Sum via iteration
    s = 0
    for i in 1:n
        s = s + m[i]
    end
    @test s == div(n * (n + 1), 2)  # n*(n+1)/2 = 5050

    # Large Float64 Memory: populate with squares
    mf = Memory{Float64}(10)
    for i in 1:10
        mf[i] = Float64(i * i)
    end
    @test mf[1] == 1.0
    @test mf[3] == 9.0
    @test mf[10] == 100.0

    # Copy of large memory
    m2 = copy(m)
    @test length(m2) == n
    @test m2[1] == 1
    @test m2[100] == 100
    # Verify independence
    m2[1] = 999
    @test m[1] == 1      # original unchanged
    @test m2[1] == 999   # copy modified
end

true
