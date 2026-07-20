using Test

# Issue #9313: a `@testset` body is a hard (local) scope (upstream Test.@testset
# wraps it in `let ... end`). A body-local binding must be discarded at block
# exit and must NOT leak into the module global scope, while an explicit `global`
# declared inside the testset binds the module global and DOES persist. Verified
# at parity with julia 1.12.
#
# `@isdefined` results are captured into locals before asserting to avoid nesting
# `@isdefined` inside `@assert` (a separate sjulia macro-in-macro-arg gap). The
# non-leakage checks use top-level `@assert` (which throws) rather than `@test`
# (which does not throw in sjulia), so the `expected = true` harness catches a
# regression.

@testset "testset-body local is discarded (Issue #9313)" begin
    inside = 42
    @test inside == 42
end
inside_defined = @isdefined(inside)
@assert !inside_defined "testset-body local leaked into module scope"

@testset "a global declared inside a testset persists (Issue #9313)" begin
    global survivor = 99
    @test survivor == 99
end
survivor_defined = @isdefined(survivor)
@assert survivor_defined "global declared inside a testset was wrongly discarded"
@assert survivor == 99

true
