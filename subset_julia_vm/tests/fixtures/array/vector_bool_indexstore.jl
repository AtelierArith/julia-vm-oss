using Test

# Test writing to Vector{Bool}(undef, n) via indexed assignment (Issue #2207)
# The IndexStore instruction must handle Bool values when writing to Bool arrays.

@testset "Vector{Bool} indexed assignment" begin
    # Basic write and read
    v = Vector{Bool}(undef, 4)
    v[1] = true
    v[2] = false
    v[3] = true
    v[4] = false
    @test v[1] == true
    @test v[2] == false
    @test v[3] == true
    @test v[4] == false

    # Overwrite values
    v[1] = false
    v[2] = true
    @test v[1] == false
    @test v[2] == true

    # typeof check
    @test typeof(v) == Vector{Bool}

    # length check
    @test length(v) == 4
end

true
