# Issue #10840 (prevention for #10771): the IR small-pure-function inliner
# must resolve a bare call inside a NESTED module (`A.B`) to `A.B`'s own
# same-named function, not to a same-named function in the parent module `A`
# or a same-named top-level function. #10771's regression test only covered a
# single level of module nesting; this fixture adds a two-level nest so a
# future regression that collapses lexical scope back to a flat/unqualified
# candidate lookup is caught even when there are THREE same-named candidates
# to choose among (top-level, `A`, and `A.B`).
#
# Each `innerNNNN` body is a trivial single-expression multiply/scale so the
# small-pure-function inliner's eligibility check (single expression body, no
# kwargs/varargs/type params) actually fires for all three candidates.

using Test

inner10840(x) = x * 1000

module A10840
    inner10840(x) = x * 100

    module B10840
        inner10840(x) = x * 2
        driver10840(n) = inner10840(n)
    end
end

@testset "nested-module bare call prefers innermost same-named function (Issue #10840)" begin
    @test A10840.B10840.driver10840(4) == 8
    @test A10840.inner10840(4) == 400
    @test inner10840(4) == 4000
end

true
