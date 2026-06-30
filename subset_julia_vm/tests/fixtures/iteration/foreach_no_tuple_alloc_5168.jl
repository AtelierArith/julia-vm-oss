# Test: generic `for x in coll` over Array/Range/Tuple/String must keep exact
# Julia semantics after the per-iteration (elem, state) tuple allocation in the
# ForEach lowering is removed (Issue #5168). Covers empty / one-element / break /
# continue / nested loops / typed-range element type / Vector{Any} mixed types.

using Test

@testset "ForEach no-tuple-alloc parity (Issue #5168)" begin
    # Array, basic sum
    arr = [1, 2, 3, 4, 5]
    total = 0
    for x in arr
        total += x
    end
    @test total == 15

    # Empty array: body must never run
    empty_arr = Int[]
    ran = 0
    for x in empty_arr
        ran += 1
    end
    @test ran == 0

    # Single element
    one = [42]
    seen = 0
    for x in one
        seen = x
    end
    @test seen == 42

    # break terminates early
    s = 0
    for x in [10, 20, 30, 40]
        s += x
        if x == 20
            break
        end
    end
    @test s == 30

    # continue skips
    s2 = 0
    for x in [1, 2, 3, 4, 5, 6]
        if x % 2 == 0
            continue
        end
        s2 += x
    end
    @test s2 == 9   # 1 + 3 + 5

    # Nested loops over arrays
    pairs = 0
    acc = 0
    for a in [1, 2, 3]
        for b in [10, 20]
            pairs += 1
            acc += a * b
        end
    end
    @test pairs == 6
    @test acc == (1 + 2 + 3) * (10 + 20)

    # Tuple iteration preserves element values and order
    t = (10, 20, 30)
    tt = 0
    for x in t
        tt += x
    end
    @test tt == 60

    # String iteration yields Char
    chars = Char[]
    for c in "ABC"
        push!(chars, c)
    end
    @test chars == ['A', 'B', 'C']

    # Range iteration
    rsum = 0
    for x in 1:5
        rsum += x
    end
    @test rsum == 15

    # Typed range preserves element type (UInt8)
    typed_count = 0
    for x in UInt8(1):UInt8(3)
        if isa(x, UInt8)
            typed_count += 1
        end
    end
    @test typed_count == 3

    # Vector{Any} with mixed element types
    mixed = Any[1, "two", 3.0, 'x']
    kinds = String[]
    for v in mixed
        push!(kinds, string(typeof(v)))
    end
    @test kinds == ["Int64", "String", "Float64", "Char"]

    # Collect via iteration into a fresh vector keeps order
    collected = Int[]
    for x in [5, 4, 3, 2, 1]
        push!(collected, x)
    end
    @test collected == [5, 4, 3, 2, 1]
end

true  # Test passed
