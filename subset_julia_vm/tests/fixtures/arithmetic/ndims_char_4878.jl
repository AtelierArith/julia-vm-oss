# Issue #4878: `ndims(::Char)` raised
# `Type error: ndims: expected array or number, got Char('A')` instead
# of returning 0. Same pattern as #4871 (`length(::Char)`): the `Char`
# carrier was missing from the builtin's scalar-arm allow-list.
# Surfaced while auditing the other scalar-aware builtins after
# PR #4877 (Issue #4875) introduced the shared `is_scalar_carrier`
# predicate.
#
# Fix: in `vm/builtins_arrays.rs`'s `Ndims` arm, replace the inline
# `matches!(... I8 | I16 | ...)` allow-list with a delegation to
# `is_scalar_carrier`, so the `Number ∪ AbstractChar` boundary stays
# in lock-step with `Length` (#4871) and `IndexLoad` (#4814).

using Test

@testset "ndims(::Char) returns 0 (Issue #4878)" begin
    @test ndims('A') == 0
    @test ndims('Z') == 0
    @test ndims('α') == 0   # multi-byte UTF-8 codepoint
    @test ndims('\n') == 0
end

@testset "ndims(::BigInt) / ndims(::BigFloat) return 0 (Issue #4878)" begin
    @test ndims(big(7)) == 0
    @test ndims(big(3.14)) == 0
end

@testset "ndims(numeric scalar) regression guard (Issue #4878)" begin
    # Number subtypes already worked before #4878; pin them so the
    # broader carrier list doesn't regress.
    @test ndims(10) == 0
    @test ndims(3.14) == 0
    @test ndims(true) == 0
    @test ndims(Int32(5)) == 0
    @test ndims(UInt8(255)) == 0
    @test ndims(Float32(1.5)) == 0
end

@testset "scalar carrier matrix: length, getindex, ndims agree (Issue #4878)" begin
    # The triple (length, getindex, ndims) all delegate to the same
    # `is_scalar_carrier` predicate after PR #4877. Pin that they
    # agree across the carrier set, so the next scalar-aware builtin
    # (`eltype`, `iterate`, `firstindex`, …) can lean on the same
    # invariant. Use `==` rather than `===` because BigInt egal has
    # an orthogonal pre-existing limitation in sjulia
    # (`big(7)[1] === big(7)` is `false`; `==` correctly returns
    # `true`).
    for x in (10, 3.14, true, 'A', Int32(5), Float32(1.5))
        @test length(x) == 1
        @test x[1] == x
        @test ndims(x) == 0
    end
end

true
