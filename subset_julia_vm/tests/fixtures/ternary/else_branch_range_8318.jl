# Issue #8318: an unparenthesized range in the else-branch of a ternary must keep
# its `:` as a range operator, i.e. `cond ? a : b:c` parses as `cond ? a : (b:c)`,
# not `(cond ? a : b):c`.

using Test

@testset "range in ternary else-branch (Issue #8318)" begin
    @test (true ? 1 : 4:6) == 1
    @test (false ? 1 : 4:6) == 4:6
    @test (true ? 10 : 20:30) == 10
    @test (false ? 10 : 20:30) == 20:30
    # else-branch is a genuine range, not the whole ternary wrapped in `:`
    @test (false ? 0 : 1:5) isa UnitRange
    @test collect(false ? 0 : 1:3) == [1, 2, 3]
end

@testset "ternary nesting unaffected (Issue #8318)" begin
    # nested ternary in the else-branch still parses (right-associative)
    @test (false ? 1 : 2 > 0 ? 5 : 6) == 5
    @test (true ? 1 : 2 > 0 ? 5 : 6) == 1
end

true
