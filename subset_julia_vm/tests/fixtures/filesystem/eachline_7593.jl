using Test

@testset "eachline(filename) vector-backed iterator (Issue #7593)" begin
    test_file = "tests/fixtures/filesystem/countlines_readline.jl"
    lines = collect(eachline(test_file))

    @test length(lines) > 10
    @test lines[1] == "# Test countlines and readline functions (Issue #482)"

    names = map(Symbol, eachline(test_file))
    @test names[1] == Symbol("# Test countlines and readline functions (Issue #482)")
end

true
