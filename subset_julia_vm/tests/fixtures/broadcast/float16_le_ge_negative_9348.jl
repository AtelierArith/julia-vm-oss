# Issue #9348: broadcast .<= / .>= over a Float16 array used to give the wrong
# result for NEGATIVE elements (0 .<= Float16[-0.5, 0.5, 1.5] yielded
# Bool[1, 1, 1]) while scalar <= and .</.> over the same data were correct, and
# Float32/Float64 broadcasts were correct. Fixed by threading Float16 through
# the element-type maps + broadcast paths (PR #9368); this fixture locks the
# negative-element Le/Ge case those changes fixed.
# Expected values parity-verified against upstream julia 1.12.

using Test

@testset "Float16 broadcast .<= / .>= with negative elements (Issue #9348)" begin
    v = Float16[-0.5, 0.5, 1.5]

    # The buggy cases: scalar .<= Float16 array / Float16 array .>= scalar.
    @test (0 .<= v) == Bool[0, 1, 1]
    @test (v .>= 0) == Bool[0, 1, 1]
    @test (0.0 .<= v) == Bool[0, 1, 1]
    @test (Float16(0) .<= v) == Bool[0, 1, 1]
    @test (v .>= Float16(0)) == Bool[0, 1, 1]
    @test (v .<= Float16(0.5)) == Bool[1, 1, 0]

    # Scalar (non-broadcast) control — was always correct.
    @test (0 <= Float16(-0.5)) == false

    # .< / .> controls over the same array — were always correct.
    @test (v .< 1) == Bool[1, 1, 0]
    @test (v .> 0) == Bool[0, 1, 1]

    # Float32 control — was always correct.
    @test (0 .<= Float32[-0.5, 0.5, 1.5]) == Bool[0, 1, 1]

    # Other Float16 array construction paths reach the same kernels.
    @test (0 .<= Float16.([-0.5, 0.5, 1.5])) == Bool[0, 1, 1]
    @test (0 .<= [Float16(-0.5), Float16(0.5)]) == Bool[0, 1]

    # 2D Float16 array.
    m = Float16[-0.5 0.5; 1.5 -1.5]
    @test (0 .<= m) == Bool[0 1; 1 0]

    # Chained broadcast comparison with a negative Float16 element (the form
    # that surfaced this bug while fixing Issue #9300).
    @test (0 .<= v .< 1) == Bool[0, 1, 0]
end

true
