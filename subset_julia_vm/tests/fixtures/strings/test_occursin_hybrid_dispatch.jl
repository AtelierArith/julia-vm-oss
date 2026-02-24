# Test hybrid dispatch for occursin: String path (Pure Julia) + Regex path (Rust builtin)
# Issue #2614: Verify both dispatch paths work correctly

using Test

@testset "occursin hybrid dispatch (Issue #2614)" begin
    @testset "String needle (Pure Julia path)" begin
        @test occursin("world", "hello world") == true
        @test occursin("xyz", "hello world") == false
        @test occursin("", "hello") == true
        @test occursin("hello", "hello") == true
        @test occursin("hello!", "hello") == false
    end

    @testset "Regex needle (Rust builtin path)" begin
        @test occursin(r"world", "hello world") == true
        @test occursin(r"^hello", "hello world") == true
        @test occursin(r"xyz", "hello world") == false
        @test occursin(r"\d+", "abc123") == true
        @test occursin(r"\d+", "abcdef") == false
    end

    @testset "Both paths produce consistent results" begin
        # Same substring test via both paths
        s = "The quick brown fox"
        @test occursin("quick", s) == occursin(r"quick", s)
        @test occursin("slow", s) == occursin(r"slow", s)
        @test occursin("fox", s) == occursin(r"fox", s)
    end
end

true
