# Test isbits and isbitstype functions

using Test

@testset "isbits/isbitstype - check if type is bits type" begin

    # isbits returns true for primitive values
    @assert isbits(42)
    @assert isbits(3.14)
    @assert isbits(true)
    @assert isbits('a')
    @assert isbits(nothing)

    # isbits returns false for non-bits values
    @assert !isbits("hello")
    @assert !isbits([1, 2, 3])

    # isbitstype returns true for primitive types
    @assert isbitstype(Int64)
    @assert isbitstype(Float64)
    @assert isbitstype(Bool)
    @assert isbitstype(Char)
    @assert isbitstype(Nothing)

    # isbitstype returns false for non-bits types
    @assert !isbitstype(String)
    @assert !isbitstype(Array)

    @test (true)
end

true  # Test passed
