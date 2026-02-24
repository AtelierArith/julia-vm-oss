# Test that collect(array) creates a proper copy (Issue #423 fix)
# Previously, collect(array) would share the reference, not copy the data

using Test

@testset "collect(array) creates independent copy for sort operations (Issue #423)" begin

    result = 0.0

    # Test 1: collect creates independent copy for Float64 arrays
    a = [1.0, 2.0, 3.0]
    b = collect(a)
    b[1] = 99.0
    if a[1] == 1.0  # a should be unchanged
        result = result + 1.0
    end
    if b[1] == 99.0  # b should have the new value
        result = result + 1.0
    end

    # Test 2: collect creates independent copy for ranges
    r = collect(1:5)
    s = collect(1:5)
    s[1] = 42
    if r[1] == 1  # r should be unchanged
        result = result + 1.0
    end

    # Test 3: sort uses collect internally (non-mutating)
    c = [3.0, 1.0, 2.0]
    d = sort(c)
    if c[1] == 3.0  # c should be unchanged after sort
        result = result + 1.0
    end
    if d[1] == 1.0  # d should be sorted
        result = result + 1.0
    end

    # Test 4: partialsort uses collect internally (non-mutating)
    e = [5.0, 3.0, 4.0, 1.0, 2.0]
    f = partialsort(e, 2)  # Get 2nd smallest element
    if e[1] == 5.0  # e should be unchanged
        result = result + 1.0
    end
    if f == 2.0  # 2nd smallest is 2.0
        result = result + 1.0
    end

    @test (result) == 7.0
end

true  # Test passed
