# Test exported reflection functions (fieldnames, fieldtypes, dump)

using Test

struct TestPoint
    x::Float64
    y::Float64
end

@testset "Reflection function exports" begin
    # fieldnames - get all field names as tuple of symbols
    names = fieldnames(TestPoint)
    @test names == (:x, :y)

    # fieldtypes - get all field types as tuple
    types = fieldtypes(TestPoint)
    @test types == (Float64, Float64)

    # dump - output object structure (just test it runs without error)
    p = TestPoint(1.0, 2.0)
    # dump returns nothing, just verify it doesn't error
    @test dump(p) === nothing
end

true
