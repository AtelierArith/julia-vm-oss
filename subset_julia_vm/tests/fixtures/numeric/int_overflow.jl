using Test

@testset "integer overflow behavior" begin
    # typemax + 1 wraps around (standard Julia behavior)
    @test typemax(Int64) + 1 == typemin(Int64)
    # typemin - 1 wraps around
    @test typemin(Int64) - 1 == typemax(Int64)
    # Verify the wrapped values are correct
    @test typemax(Int64) + 1 == 0 - 9223372036854775807 - 1
    @test typemin(Int64) - 1 == 9223372036854775807
end

true
