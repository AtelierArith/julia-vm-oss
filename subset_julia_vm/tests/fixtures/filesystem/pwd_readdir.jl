# Test pwd() and readdir() functions
# pwd() returns current working directory
# readdir(path) returns sorted list of directory contents

using Test

@testset "pwd and readdir functions (Issue #457)" begin
    # Test pwd() returns a non-empty string
    cwd = pwd()
    @test isa(cwd, String)
    @test length(cwd) > 0

    # Test that pwd() returns an absolute path (starts with /)
    @test startswith(cwd, "/")

    # Test readdir() on current directory
    entries = readdir(".")
    @test isa(entries, Vector)
    @test length(entries) >= 0

    # Test readdir with explicit path
    entries2 = readdir(cwd)
    @test isa(entries2, Vector)
end

true
