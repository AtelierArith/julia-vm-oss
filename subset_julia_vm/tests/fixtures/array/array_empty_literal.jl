# Test empty array literals: [] and Type[]

using Test

@testset "Empty array literals: [] and Type[]" begin

    # === Untyped empty array [] ===
    empty_any = []
    @assert length(empty_any) == 0

    # Can push elements to empty array
    push!(empty_any, 1)
    push!(empty_any, 2)
    @assert length(empty_any) == 2
    @assert empty_any[1] == 1
    @assert empty_any[2] == 2

    # === Typed empty array Int64[] ===
    empty_int = Int64[]
    @assert length(empty_int) == 0

    # Can push elements
    push!(empty_int, 10)
    push!(empty_int, 20)
    push!(empty_int, 30)
    @assert length(empty_int) == 3
    @assert sum(empty_int) == 60

    # === Typed empty array Float64[] ===
    empty_float = Float64[]
    @assert length(empty_float) == 0

    push!(empty_float, 1.5)
    push!(empty_float, 2.5)
    @assert length(empty_float) == 2
    @assert sum(empty_float) == 4.0

    # === Typed empty array Bool[] ===
    empty_bool = Bool[]
    @assert length(empty_bool) == 0

    push!(empty_bool, true)
    push!(empty_bool, false)
    @assert length(empty_bool) == 2
    @assert empty_bool[1] == true
    @assert empty_bool[2] == false

    # === Typed empty array String[] ===
    empty_str = String[]
    @assert length(empty_str) == 0

    push!(empty_str, "hello")
    push!(empty_str, "world")
    @assert length(empty_str) == 2

    # All tests passed
    @test (true)
end

true  # Test passed
