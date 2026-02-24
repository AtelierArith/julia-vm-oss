# Test: Dict iteration in for loops (Issue #1114)
# In Julia, iterating over a Dict yields Pair(key, value) for each entry.

using Test

@testset "Dict iteration" begin
    # Basic Dict iteration
    d = Dict("a" => 1, "b" => 2, "c" => 3)

    # Collect keys and values via iteration
    keys_collected = String[]
    vals_collected = Int[]
    for pair in d
        push!(keys_collected, pair.first)
        push!(vals_collected, pair.second)
    end

    # Check that we got all entries (order may vary)
    @test length(keys_collected) == 3
    @test length(vals_collected) == 3
    @test "a" in keys_collected
    @test "b" in keys_collected
    @test "c" in keys_collected
    @test 1 in vals_collected
    @test 2 in vals_collected
    @test 3 in vals_collected

    # Empty Dict iteration
    empty_d = Dict{String,Int}()
    count = 0
    for _ in empty_d
        count = count + 1
    end
    @test count == 0

    # Sum values via iteration
    d2 = Dict(1 => 10, 2 => 20, 3 => 30)
    total = 0
    for pair in d2
        total = total + pair.second
    end
    @test total == 60
end

true
