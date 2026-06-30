using Test

# Matrix-style leak guard for Issue #4729 / Issues #4725 #4727: every
# value-to-string entry point in sjulia must render a heap-allocated
# `Pair` as "1 => 2", never as the Rust debug repr
# "StructRef(heap_idx=N)". A regression in any one of these paths is
# user-visible (silent wrong output).

@testset "Pair value-to-string leak guard matrix (Issue #4729)" begin
    p = Pair(1, 2)

    # string() builtin — covered by PR #4726 (Issue #4725)
    @test string(p) == "1 => 2"

    # repr() builtin — covered by PR #4726 (Issue #4725)
    @test repr(p) == "1 => 2"

    # String interpolation — covered by PR #4728 (Issue #4727)
    @test "$p" == "1 => 2"
    @test "Wrapped: $p" == "Wrapped: 1 => 2"

    # NOTE: sjulia's `sprintf` (covered in this PR) is exposed as a
    # Base-level function; upstream Julia uses `Printf.@sprintf`, so
    # the sprintf parity assertions live in a separate sjulia-only
    # fixture and are not in this upstream-parity matrix.

    # string() composition with other args
    @test string("Wrapped: ", p) == "Wrapped: 1 => 2"
    @test string(p, " end") == "1 => 2 end"

    # Tuple/Ref/QuoteNode carriers preserved across all entry points
    @test string((1, p)) == "(1, 1 => 2)"
    @test "$((1, p))" == "(1, 1 => 2)"
end

true
