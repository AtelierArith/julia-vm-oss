# Issue #5082: iterating a NamedTuple yields its field values in order.
# Loop-variable element-type inference now propagates the field value type
# (Union of all field value types) instead of widening to Top, so the loop
# body's arithmetic can stay specialized. This fixture verifies the runtime
# values match upstream Julia.

using Test

# Homogeneous NamedTuple: every field is Int64, so `v::Int64` inside the loop.
function sum_named_tuple(nt)
    s = 0
    for v in nt
        s += v
    end
    return s
end

# Homogeneous Float64 NamedTuple.
function sum_named_tuple_float(nt)
    acc = 0.0
    for v in nt
        acc += v
    end
    return acc
end

# Heterogeneous NamedTuple (Int64 and Float64 fields): iteration yields the
# union of value types; promoted arithmetic still works.
function sum_mixed_named_tuple(nt)
    acc = 0.0
    for v in nt
        acc += v
    end
    return acc
end

@testset "Issue #5082: NamedTuple for-loop iterates field values" begin
    @test sum_named_tuple((a = 1, b = 2, c = 3)) == 6
    @test sum_named_tuple_float((x = 1.0, y = 2.5, z = 3.5)) == 7.0
    @test sum_mixed_named_tuple((a = 1, b = 2.5)) == 3.5
end

true  # Test passed
