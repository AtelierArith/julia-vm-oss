# Test nfields

using Test

struct Point3D
    x::Float64
    y::Float64
    z::Float64
end

@testset "nfields - number of fields in a struct (returns Int64)" begin

    p = Point3D(1.0, 2.0, 3.0)

    # nfields should return 3 (Int64)
    @test (nfields(p)) == 3
end

true  # Test passed
