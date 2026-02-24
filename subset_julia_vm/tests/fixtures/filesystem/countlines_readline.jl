# Test countlines and readline functions (Issue #482)

using Test

@testset "countlines and readline functions (Issue #482)" begin
    # Use existing test files - construct path from pwd
    # Tests typically run from subset_julia_vm directory

    # Test with the manifest file in the current test directory
    # The path is relative to where cargo test runs from
    test_manifest = "tests/fixtures/filesystem/manifest.toml"

    # Test countlines - count lines in manifest file
    n = countlines(test_manifest)
    @test n > 5  # manifest has multiple test entries
    @test isa(n, Int64)

    # Test readline - read first line from manifest
    first_line = readline(test_manifest)
    @test isa(first_line, String)
    @test length(first_line) > 0  # First line should have content
    # First line should be a comment or section header
    @test startswith(first_line, "#") || startswith(first_line, "[[")

    # Test with another known file - the test file itself
    test_file = "tests/fixtures/filesystem/countlines_readline.jl"
    n2 = countlines(test_file)
    @test n2 > 10  # This file has more than 10 lines

    first_jl = readline(test_file)
    @test contains(first_jl, "Test") || contains(first_jl, "#")
end

true
