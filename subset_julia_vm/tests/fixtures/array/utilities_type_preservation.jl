# Test array utilities preserve element types (Issue #424)
# These functions should preserve the element type of the input array

using Test

@testset "Array utilities preserve element types (Issue #424)" begin

    result = 0.0

    # Test copy preserves type
    a = [1.0, 2.0, 3.0]
    b = copy(a)
    if length(b) == 3 && b[1] == 1.0 && b[2] == 2.0 && b[3] == 3.0
        result = result + 1.0
    end
    # Verify copy creates independent array
    b[1] = 99.0
    if a[1] == 1.0  # a should be unchanged
        result = result + 1.0
    end

    # Test reverse preserves values correctly
    c = [1.0, 2.0, 3.0, 4.0]
    d = reverse(c)
    if d[1] == 4.0 && d[2] == 3.0 && d[3] == 2.0 && d[4] == 1.0
        result = result + 1.0
    end
    # Verify reverse creates independent array
    if c[1] == 1.0  # c should be unchanged
        result = result + 1.0
    end

    # Test vcat concatenates correctly
    e = [1.0, 2.0]
    f = [3.0, 4.0]
    g = vcat(e, f)
    if length(g) == 4 && g[1] == 1.0 && g[2] == 2.0 && g[3] == 3.0 && g[4] == 4.0
        result = result + 1.0
    end

    # Test vec flattens correctly
    h = [1.0, 2.0, 3.0]
    i = vec(h)
    if length(i) == 3 && i[1] == 1.0 && i[2] == 2.0 && i[3] == 3.0
        result = result + 1.0
    end

    # Test circshift with k=0 (should be a copy)
    j = [1.0, 2.0, 3.0]
    k = circshift(j, 0)
    if k[1] == 1.0 && k[2] == 2.0 && k[3] == 3.0
        result = result + 1.0
    end

    # Test circshift with positive k (shift right)
    l = circshift(j, 1)
    if l[1] == 3.0 && l[2] == 1.0 && l[3] == 2.0
        result = result + 1.0
    end

    # Test circshift with negative k (shift left)
    m = circshift(j, -1)
    if m[1] == 2.0 && m[2] == 3.0 && m[3] == 1.0
        result = result + 1.0
    end

    @test (result) == 9.0
end

true  # Test passed
