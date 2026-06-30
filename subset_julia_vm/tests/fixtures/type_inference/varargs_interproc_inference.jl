# Test: Interprocedural inference packs varargs parameters as Tuples (Issue #3526)
# A user-defined varargs function should be analyzed with `xs` bound as
# `Tuple{Int64, Int64, Int64}` so that `for x in xs` and `s += x` infer Int64.
using Test

sum_varargs(xs...) = begin
    s = 0
    for x in xs
        s += x
    end
    s
end

function call_sum_varargs()
    sum_varargs(1, 2, 3)
end

function call_sum_varargs_one()
    sum_varargs(42)
end

function call_sum_varargs_zero()
    sum_varargs()
end

@testset "Varargs interprocedural inference" begin
    @test call_sum_varargs() == 6
    @test call_sum_varargs_one() == 42
    @test call_sum_varargs_zero() == 0
end

true
