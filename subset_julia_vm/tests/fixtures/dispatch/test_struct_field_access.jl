# Test dynamic field access on Any-typed struct values

using Test

struct Point
    x::Int64
    y::Int64
end

function get_x(val)
    val.x
end

function get_y(val)
    val.y
end

function get_field_names(x)
    fieldnames(typeof(x))
end

@testset "Dynamic field access on Any-typed struct values (Issue #401)" begin


    # Direct field access works
    p = Point(10, 20)
    check1 = p.x == 10 && p.y == 20

    # Function that receives struct as Any


    # Test field access through Any-typed parameter
    check2 = get_x(p) == 10
    check3 = get_y(p) == 20

    # Test fieldnames via typeof (Julia requires type, not instance)
    names = fieldnames(typeof(p))
    check4 = length(names) == 2

    # Test fieldnames through Any-typed function parameter
    names2 = get_field_names(p)
    check5 = length(names2) == 2

    @test (check1 && check2 && check3 && check4 && check5)
end

true  # Test passed
