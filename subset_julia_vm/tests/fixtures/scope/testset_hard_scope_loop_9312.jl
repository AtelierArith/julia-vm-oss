using Test

# Issue #9312: `@testset` must wrap its body in a hard (local) scope, matching
# upstream `Test.@testset` (stdlib/Test/src/Test.jl `testset_beginend_call`,
# which wraps the body in `let ... end`). A `for`/`while` loop that accumulates
# into a variable defined in the testset body must update that testset-local
# instead of raising `UndefVarError` under file-mode soft-scope localization
# (the body assignment previously looked like a module global, so the loop
# `acc += k` was localized to a fresh, undefined loop-local).
#
# Issue #9313 (now fixed) additionally verifies that these testset-body
# accumulators are hard-scope locals: they must be discarded at block exit and
# must NOT leak into the module global scope (asserted at the end below), exactly
# as upstream isolates them.

@testset "for-loop accumulator into testset-local (Issue #9312)" begin
    acc = 0
    for k in 1:3
        acc += k
    end
    @test acc == 6
end

@testset "while-loop accumulator into testset-local" begin
    total = 0
    i = 1
    while i <= 4
        total += i
        i += 1
    end
    @test total == 10
end

@testset "nested loops accumulate into testset-local" begin
    s = 0
    for a in 1:3
        for b in 1:2
            s += a * b
        end
    end
    @test s == 18
end

@testset "loop building a testset-local array" begin
    xs = Int[]
    for k in 1:5
        push!(xs, k * k)
    end
    @test xs == [1, 4, 9, 16, 25]
end

# Issue #9313: none of the testset-body accumulators leak into module scope.
# (`@isdefined` results are captured into locals to avoid nesting `@isdefined`
# inside `@assert`; top-level `@assert` throws — unlike `@test` — so the
# `expected = true` harness catches a leakage regression.)
acc_leaked = @isdefined(acc)
total_leaked = @isdefined(total)
s_leaked = @isdefined(s)
xs_leaked = @isdefined(xs)
@assert !acc_leaked "testset for-loop accumulator leaked into module scope"
@assert !total_leaked "testset while-loop accumulator leaked into module scope"
@assert !s_leaked "testset nested-loop accumulator leaked into module scope"
@assert !xs_leaked "testset loop-built array leaked into module scope"

true
