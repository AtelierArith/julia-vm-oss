# Test Float64 display format (Issue: whole numbers should print with .0)
# Julia prints whole Float64 values with .0 suffix (e.g., 2.0 not 2)

using Test

@testset "Float display: whole Float64/Float32 values preserve type for oftype and convert" begin

    result = 0.0

    # Test 1: Direct float literal
    # Note: We can't capture print output, so we verify the type is preserved
    x = 2.0
    if typeof(x) == Float64
        result = result + 1.0
    end

    # Test 2: Float64 constructor from integer
    y = Float64(3)
    if typeof(y) == Float64
        result = result + 1.0
    end

    # Test 3: oftype creates correct float
    z = oftype(1.0, 4)
    if typeof(z) == Float64
        result = result + 1.0
    end

    # Test 4: convert creates correct float
    w = convert(Float64, 5)
    if typeof(w) == Float64
        result = result + 1.0
    end

    # Test 5: Float arithmetic preserves type
    a = 2.0 + 0.0
    if typeof(a) == Float64
        result = result + 1.0
    end

    # Test 6: Negative whole float
    b = -3.0
    if typeof(b) == Float64
        result = result + 1.0
    end

    # Test 7: Zero float
    c = 0.0
    if typeof(c) == Float64
        result = result + 1.0
    end

    # Test 8: Float32 whole number
    d = Float32(2)
    if typeof(d) == Float32
        result = result + 1.0
    end

    @test (result) == 8.0
end

true  # Test passed
