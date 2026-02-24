# Test array literal type inference (Issue #609)
# Array literals should create arrays with the correct element type

using Test

@testset "Array literals create arrays with correct element types (Issue #609)" begin

    result = 0.0

    # Test 1: Integer array literal creates Int64 array
    a = [1, 2, 3]
    if eltype(a) == Int64
        result = result + 1.0
    end

    # Test 2: Float array literal creates Float64 array
    b = [1.0, 2.0, 3.0]
    if eltype(b) == Float64
        result = result + 1.0
    end

    # Test 3: Mixed int/float creates Float64 array (promotion)
    c = [1, 2.0, 3]
    if eltype(c) == Float64
        result = result + 1.0
    end

    # Test 4: Bool array creates Bool array
    d = [true, false, true]
    if eltype(d) == Bool
        result = result + 1.0
    end

    # Test 5: Element access from Int64 array returns Int64
    x = a[1]
    if typeof(x) == Int64
        result = result + 1.0
    end

    # Test 6: Element access from Float64 array returns Float64
    y = b[1]
    if typeof(y) == Float64
        result = result + 1.0
    end

    # Test 7: Element assignment preserves array type (Int64)
    a[1] = 5
    if eltype(a) == Int64
        result = result + 1.0
    end

    # Test 8: Verify the assigned value
    if a[1] == 5
        result = result + 1.0
    end

    @test (result) == 8.0
end

true  # Test passed
