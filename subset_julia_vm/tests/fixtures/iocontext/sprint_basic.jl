using Test

@testset "sprint basic usage" begin
    @testset "sprint with print" begin
        # sprint captures output of print (no type-specific show dispatch needed)
        @test sprint(print, "hello") == "hello"
        @test sprint(print, 42) == "42"
        @test sprint(print, 3.14) == "3.14"
        @test sprint(print, true) == "true"
        @test sprint(print, false) == "false"
    end

    @testset "sprint with show" begin
        # sprint(show, x) uses type-specific show dispatch (Issue #3120 fixed)
        # show adds quotes around strings, unlike print
        @test sprint(show, 42) == "42"
        @test sprint(show, 3.14) == "3.14"
        @test sprint(show, true) == "true"
        @test sprint(show, false) == "false"
        @test sprint(show, "hello") == "\"hello\""
    end

    @testset "sprint with println" begin
        # println adds newline
        @test sprint(println, "hello") == "hello\n"
        @test sprint(println, 42) == "42\n"
    end

    @testset "sprint captures multi-argument output" begin
        # sprint can capture multi-argument print calls
        @test sprint(print, "x = ", 42) == "x = 42"
        @test sprint(print, 1, " + ", 1, " = ", 2) == "1 + 1 = 2"
    end

    @testset "string() as show equivalent" begin
        # string() converts values to their display representation
        @test string(42) == "42"
        @test string(3.14) == "3.14"
        @test string(true) == "true"
    end
end

true
