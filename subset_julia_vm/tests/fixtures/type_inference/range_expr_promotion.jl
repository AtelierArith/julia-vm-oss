# Test: range expression promotes mixed numeric endpoints/step rather than
# falling back to Range{Int64} (Issue #3519).

using Test

# Mixed Int64 endpoints with a Float64 step: range should iterate Float64s.
function f_mixed_step()
    r = 1:0.5:2
    x = first(r)
    y = last(r)
    x + y
end

# 1.0:2 (Int upper bound) should still produce a Float64 range.
function f_float_int()
    r = 1.0:2
    first(r) + last(r)
end

# Pure Int range stays Int (regression guard).
function f_int_int()
    r = 1:2
    first(r) + last(r)
end

@testset "Range expression element promotion (Issue #3519)" begin
    @test f_mixed_step() == 3.0
    @test f_mixed_step() isa Float64

    @test f_float_int() == 3.0
    @test f_float_int() isa Float64

    @test f_int_int() == 3
    @test f_int_int() isa Int
end

true  # Test passed
