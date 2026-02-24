using Test

# Issue #3070: try/catch type-widening regression test
# Variables assigned in both try and catch blocks should have widened types

@testset "try/catch variable assigned in both branches" begin
    x = 0
    try
        x = 10
    catch
        x = -1
    end
    @test x == 10
end

@testset "try/catch with thrown exception" begin
    x = 0
    try
        error("fail")
        x = 10
    catch
        x = -1
    end
    @test x == -1
end

@testset "try/catch mixed type assignment" begin
    val = 0
    try
        val = 42
    catch
        val = -1
    end
    @test val == 42
end

@testset "try/catch with finally" begin
    result = 0
    cleanup = false
    try
        result = 100
    catch
        result = -100
    finally
        cleanup = true
    end
    @test result == 100
    @test cleanup == true
end

@testset "try/catch exception path with finally" begin
    result = 0
    cleanup = false
    try
        error("oops")
        result = 100
    catch
        result = -100
    finally
        cleanup = true
    end
    @test result == -100
    @test cleanup == true
end

@testset "nested try/catch widening" begin
    outer = 0
    inner = 0
    try
        try
            error("inner fail")
        catch
            inner = -1
        end
        outer = 1
    catch
        outer = -1
    end
    @test inner == -1
    @test outer == 1
end

@testset "try/catch with specific exception type" begin
    msg = ""
    try
        error("test message")
    catch e
        msg = e.msg
    end
    @test msg == "test message"
end

true
