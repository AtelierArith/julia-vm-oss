using Test

# Regression test for Issue #3592:
# `stack([[1,2], [3]])` (ragged input) must raise a DimensionMismatch-style
# user-facing error before any indexing happens, rather than leaking an
# internal "Index [N] out of bounds" runtime error.

@testset "stack ragged validation (#3592)" begin
    # Ragged: shorter second slice — should raise an ErrorException with a
    # dimension-mismatch message (Julia raises DimensionMismatch; we use
    # error(...) since the VM's exception system is simpler).
    @test_throws Exception stack([[1, 2], [3]])

    # Ragged: longer second slice
    @test_throws Exception stack([[1], [2, 3]])

    # Uniform: still works
    m = stack([[1, 2], [3, 4]])
    @test size(m) == (2, 2)
    @test m[1, 1] == 1
    @test m[2, 1] == 2
    @test m[1, 2] == 3
    @test m[2, 2] == 4

    # Single slice: trivially uniform
    s = stack([[1, 2, 3]])
    @test size(s) == (3, 1)
end

true
