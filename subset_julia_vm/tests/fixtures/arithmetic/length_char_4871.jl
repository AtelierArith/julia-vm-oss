# Issue #4871: `length(::Char)` raised
# `Type error: length not defined for Char('A')` instead of returning
# `1`. Surfaced as a sibling pre-existing limitation while fixing
# scalar `getindex` in PR #4870 (Issue #4814).
#
# After PR #4870, `'A'[1] == 'A'` works; the symmetric
# `length('A') == 1` query was still rejected, breaking the upstream
# Julia invariant `length(x) == 1 ⟺ x[1] is valid` for scalar `x`
# from the `Number ∪ AbstractChar` hierarchy.
#
# Fix: in `subset_julia_vm/src/vm/builtins_collections.rs`'s `Length`
# arm, add `Value::Char(_)`, `Value::BigInt(_)`, and
# `Value::BigFloat(_)` to the list of scalar carriers that resolve
# to `length == 1`. Matches the same `Number ∪ AbstractChar`
# boundary `is_scalar_indexable_value` (PR #4870) uses for
# `IndexLoad`.

using Test

@testset "length(::Char) returns 1 (Issue #4871)" begin
    @test length('A') == 1
    @test length('Z') == 1
    @test length('α') == 1   # multi-byte UTF-8 codepoint
    @test length('\n') == 1
end

@testset "length(::BigInt) / length(::BigFloat) return 1 (Issue #4871)" begin
    @test length(big(7)) == 1
    @test length(big(3.14)) == 1
    @test length(big(0)) == 1
end

@testset "length(numeric scalar) regression guard (Issue #4871)" begin
    # Number subtypes already worked before #4871 — pin them so the
    # broader scalar-carrier list doesn't regress when extended.
    @test length(10) == 1
    @test length(3.14) == 1
    @test length(true) == 1
    @test length(Int32(5)) == 1
    @test length(UInt8(255)) == 1
    @test length(Float32(1.5)) == 1
end

@testset "length(scalar) pairs with getindex(scalar, 1) (Issue #4871)" begin
    # The pre-#4870 / pre-#4871 mismatch was: scalar `getindex`
    # rejected but `length` accepted, or vice-versa for `Char`.
    # Both should now agree across the `Number ∪ AbstractChar`
    # boundary.
    for x in (10, 3.14, true, 'A', Int32(5))
        @test length(x) == 1
        @test x[1] == x
    end
end

true
