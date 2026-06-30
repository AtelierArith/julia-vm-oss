# Test that long homogeneous varargs calls behave correctly under inference
# normalization (Issue #3511). The lattice now collapses long flat-tuple
# argtypes for varargs calls to a `Tuple{T, Vararg{T}}` shape, which keeps
# inference cache keys bounded — runtime behavior must remain unchanged.

using Test

sum_all(args...) = begin
    total = 0
    for x in args
        total += x
    end
    total
end

mul_int(x::Int64, ys::Int64...) = begin
    p = x
    for y in ys
        p = p * y
    end
    p
end

@testset "Long homogeneous varargs (Issue #3511)" begin
    # 16 Int64 arguments — exceeds the inference normalization threshold
    # (TUPLE_VARARG_NORMALIZE_THRESHOLD = 8). Behavior must be identical
    # to the official Julia interpreter.
    long_call = sum_all(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16)
    @test long_call == 136

    # Typed-prefix variant exercises the bind-with-vararg path.
    @test mul_int(1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1) == 1
    @test mul_int(2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1) == 2

    # Short call still works — the normalization is length-gated.
    @test sum_all(1, 2, 3) == 6
    @test mul_int(3, 2) == 6
end

true
