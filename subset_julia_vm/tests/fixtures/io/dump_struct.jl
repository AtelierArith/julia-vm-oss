# Test dump() for user-defined structs with nested field introspection (Issue #370)

using Test

# Define test structs outside @testset
struct MyStruct
    a::Int64
    b::Tuple{Int64, Int64}
end

struct Point
    x::Float64
    y::Float64
end

struct NestedStruct
    name::String
    point::Point
end

@testset "dump() for user-defined structs" begin
    # Test basic struct dump - should show type name and field values
    s = MyStruct(1, (2, 3))

    # dump() should show the struct type and recursively show fields
    # Note: We test that dump doesn't error and returns nothing
    result = dump(s)
    @test result === nothing

    # Test simple struct
    p = Point(1.0, 2.0)
    result2 = dump(p)
    @test result2 === nothing

    # Test nested struct
    ns = NestedStruct("test", Point(3.0, 4.0))
    result3 = dump(ns)
    @test result3 === nothing

    # Test field access works correctly for structs
    @test s.a == 1
    @test s.b == (2, 3)

    @test p.x == 1.0
    @test p.y == 2.0

    # Test nested field access
    @test ns.name == "test"
    @test ns.point.x == 3.0
    @test ns.point.y == 4.0
end

true
