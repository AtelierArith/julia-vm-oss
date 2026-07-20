# Follow-up of #9942 (PR #9950): two adjacent module-body function-collection
# gaps (Issue #10073).
#
# (1) `collect_from_module` only descended into a module-body `LetBlock` when
#     it carried the `_testset_begin!` marker left by `Test.@testset`
#     expansion, so a function defined inside a PLAIN (non-testset) `let` at
#     module scope was never registered -> `Unknown function`.
# (2) Helpers that WERE collected (e.g. inside `@testset`) registered at
#     `None`/Main scope instead of the enclosing module, so a helper
#     referencing a module-scope global raised `UndefVarError`.
#
# Functions defined inside a `let`/`@testset` are local to that hard scope in
# upstream Julia (not accessible as `Module.helper` from outside), so results
# are captured through a `Ref` mutated from inside the scope, matching how
# upstream code observes such locals.
#
# `MPlainLet` below exercises (1); `MTestsetGlobal` exercises (2); `MNested`
# exercises both together (a plain `let` nested inside a `@testset`, with a
# helper closing over both the let-local and a module global).
module MPlainLet

r = let
    k(x) = x + 1
    k(1)
end

end

module MTestsetGlobal
using Test

G = 10
RESULT = Ref(0)

@testset "helper sees module global" begin
    f(x) = x + G
    v = f(1)
    RESULT[] = v
    @test v == 11
end

end

module MNested
using Test

G2 = 100
RESULT = Ref(0)

@testset "nested let, helper sees let-local and module global" begin
    let
        local_offset = 5
        h(x) = x + local_offset + G2
        v = h(1)
        RESULT[] = v
        @test v == 106
    end
end

end

MPlainLet.r == 2 &&
    MTestsetGlobal.RESULT[] == 11 &&
    MNested.RESULT[] == 106 &&
    true
