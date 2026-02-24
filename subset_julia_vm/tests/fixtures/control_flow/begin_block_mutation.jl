using Test

# Test: mutations inside begin...end blocks are not dropped (Issue #3102)
@testset "begin...end block mutation side effects" begin
    # Array mutations across multiple statements in a begin block
    arr = [0, 0, 0]
    begin
        arr[1] = 10
        arr[2] = 20
        arr[3] = 30
    end
    @test arr[1] == 10
    @test arr[2] == 20
    @test arr[3] == 30

    # Counter accumulation: all statements must execute
    total = 0
    begin
        total = total + 10
        total = total + 20
        total = total + 5
    end
    @test total == 35

    # Variable declarations across two separate begin blocks
    begin
        v1 = 1
        v2 = 2
        v3 = 3
    end
    @test v1 + v2 + v3 == 6

    # Return value is last expression; earlier assignments still take effect
    n = 0
    result = begin
        n = 100
        m = 200
        n + m
    end
    @test result == 300
    @test n == 100
end

true
