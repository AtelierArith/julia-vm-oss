# Test tryparse function - parse string with nothing on failure

using Test

@testset "tryparse(T, s) - parse string, return nothing on failure" begin

    # === Parse Int64 ===
    @assert tryparse(Int64, "123") == 123
    @assert tryparse(Int64, "-456") == -456
    @assert tryparse(Int64, "0") == 0
    @assert tryparse(Int64, "  789  ") == 789  # trimmed

    # === Parse Int64 failures ===
    @assert tryparse(Int64, "abc") === nothing
    @assert tryparse(Int64, "12.34") === nothing
    @assert tryparse(Int64, "") === nothing
    @assert tryparse(Int64, "   ") === nothing

    # === Parse Float64 ===
    @assert tryparse(Float64, "3.14") == 3.14
    @assert tryparse(Float64, "-2.5") == -2.5
    @assert tryparse(Float64, "0.0") == 0.0
    @assert tryparse(Float64, "  1.5  ") == 1.5  # trimmed
    @assert tryparse(Float64, "42") == 42.0  # int to float

    # === Parse Float64 failures ===
    @assert tryparse(Float64, "abc") === nothing
    @assert tryparse(Float64, "") === nothing

    # === Using Int alias ===
    @assert tryparse(Int, "100") == 100
    @assert tryparse(Int, "xyz") === nothing

    # All tests passed
    @test (true)
end

true  # Test passed
