using Test

struct PairBox4847
    a
    b
end

# Both branches of the ternary build the same immutable struct shape, so the
# constructor field facts must be joined and survive the surrounding
# `getfield`, recovering `String` rather than widening to `Any` (Issue #4847).
use_box_branch4847(flag) = getfield(flag ? PairBox4847(1, "x") : PairBox4847(2, "y"), :b)

# Same idea via an if-else expression bound to a local before `getfield`.
function use_box_ifelse4847(flag)
    box = if flag
        PairBox4847(1, "x")
    else
        PairBox4847(2, "y")
    end
    return getfield(box, :b)
end

# Differing field types across branches must lattice-join (Int64), not be lost.
use_box_branch_int4847(flag) = getfield(flag ? PairBox4847(1, "x") : PairBox4847(2, "y"), :a)

@testset "PartialStruct branch-join return field inference (Issue #4847)" begin
    @test use_box_branch4847(true) == "x"
    @test use_box_branch4847(false) == "y"
    @test use_box_ifelse4847(true) == "x"
    @test use_box_branch_int4847(true) == 1

    @test Base.infer_return_type(use_box_branch4847, Tuple{Bool}) == String
    @test Base.return_types(use_box_branch4847, Tuple{Bool})[1] == String
    @test Base.infer_return_type(use_box_ifelse4847, Tuple{Bool}) == String
    @test Base.return_types(use_box_ifelse4847, Tuple{Bool})[1] == String
    @test Base.infer_return_type(use_box_branch_int4847, Tuple{Bool}) == Int64
    @test Base.return_types(use_box_branch_int4847, Tuple{Bool})[1] == Int64
end

true
