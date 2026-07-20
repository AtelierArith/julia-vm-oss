# eval(expr) evaluates in the module where the call appears, matching
# upstream's per-module `eval(x) = Core.eval(M, x)` definition: a bare
# `eval(:(x = 1))` inside module P.C installs `P.C.x`, never `Main.x`.
# The explicit `Core.eval(m, expr)` / `Base.eval(m, expr)` forms target
# the given module (Issue #11421, tech-debt #11448).
using Test

# eval at Main top level keeps installing Main globals.
eval(:(eval_main_x_11421 = 10))

module EvalP11421
module C
eval(:(x = 1))
end
end

module EvalQ11421
w = 5
doubled = eval(:(w + w))
end

module EvalR11421
@eval r_val = 7
end

module EvalT11421
g() = eval(:(t_global = 99))
end

@testset "bare eval targets the enclosing module (Issue #11421)" begin
    @test eval_main_x_11421 == 10
    @test isdefined(EvalP11421.C, :x)
    @test !isdefined(Main, :x)
    @test EvalP11421.C.x == 1

    # eval reads existing globals of its own module
    @test EvalQ11421.doubled == 10

    # @eval expands to the same per-module eval
    @test isdefined(EvalR11421, :r_val)
    @test EvalR11421.r_val == 7
    @test !isdefined(Main, :r_val)

    # eval inside a function defined in a module targets that module
    EvalT11421.g()
    @test isdefined(EvalT11421, :t_global)
    @test !isdefined(Main, :t_global)
end

@testset "explicit module-target eval (Issue #11421)" begin
    Core.eval(EvalP11421.C, :(y = 2))
    @test EvalP11421.C.y == 2
    Base.eval(EvalP11421.C, :(z = 3))
    @test EvalP11421.C.z == 3
    Core.eval(Main, :(eval_main_y_11421 = 20))
    @test eval_main_y_11421 == 20
end

true
