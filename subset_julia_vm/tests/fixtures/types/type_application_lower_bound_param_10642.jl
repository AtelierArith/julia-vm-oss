# Literal type application `T{S}` on a user parametric type whose parameter
# declares a LOWER bound (`S>:Lo`) accepts exactly the types S with Lo <: S
# (the lower bound itself and its proper supertypes) and rejects everything
# else with TypeError, matching upstream jl_apply_type bound validation. The
# literal path used to test the bound in the wrong direction and reject valid
# supertypes (`LB{Signed}`, `LB{Integer}`); the Core.apply_type path was the
# over-permissive mirror (#10554). Regression coverage for Issue #10642.

using Test

struct LBHolder10642{S>:Int32} end

make_lb_10642() = LBHolder10642{Signed}

@testset "literal T{S} on a lower-bounded parameter (Issue #10642)" begin
    # The bound itself is trivially valid.
    @test LBHolder10642{Int32} === LBHolder10642{Int32}

    # Proper supertypes of the lower bound are valid.
    @test LBHolder10642{Signed} === LBHolder10642{Signed}
    @test LBHolder10642{Integer} === LBHolder10642{Integer}
    @test LBHolder10642{Any} === LBHolder10642{Any}

    # Types that are NOT supertypes of Int32 raise TypeError.
    @test_throws TypeError LBHolder10642{Int64}
    @test_throws TypeError LBHolder10642{Int16}
    @test_throws TypeError LBHolder10642{Union{}}

    # Same result from inside a function body.
    @test make_lb_10642() === LBHolder10642{Signed}

    # The Core.apply_type spelling validates the same direction (#10554).
    @test Core.apply_type(LBHolder10642, Signed) === LBHolder10642{Signed}
    @test_throws TypeError Core.apply_type(LBHolder10642, Int64)
end

true
