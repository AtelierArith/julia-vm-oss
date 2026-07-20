# Comprehension over an inline range: element type joined across start/step/stop
# (Issue #9293, sibling of the Stmt::For fix for Issue #9291).
#
# `compile_comprehension_with_elem_inner` used to infer the loop variable's
# element type from the range `start` expression ALONE. For `0:0.5:6` the start
# `0` pinned the loop var to `Int64` while the runtime elements are `Float64`, so
# the element store failed with `Type error: expected I64, got "Float64"`. The
# element type must be joined across start/step/stop: a float anywhere ⇒ float
# elements; an `Any`-inferred step ⇒ the runtime-typed (typejoin) path.

using Test

@testset "Comprehension inline range elem type (Issue #9293)" begin
    # --- The three MWEs that errored before the fix (julia gives 13) ---

    # inline float literal step: Int start + Float64 step ⇒ Float64 elements
    v1 = [u for u in 0:0.5:6]
    @test length(v1) == 13
    @test eltype(v1) == Float64
    @test v1 == [0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0, 4.5, 5.0, 5.5, 6.0]

    # computed float step (a BinaryOp, not a bare float literal)
    @test length([u for u in 0:(2π / 12):2π]) == 13

    # Any-typed step via an unannotated function parameter
    fstep(st) = length([u for u in 0:st:6])
    @test fstep(0.5) == 13

    # --- The three control cases from the #9293 table (already worked) ---

    # float start pins the element type to Float64 directly
    fstart(st) = length([u for u in 0.0:st:6])
    @test fstart(0.5) == 13

    # range bound to a local first takes the non-inline iterator path
    flocal(st) = (r = 0:st:6; length([u for u in r]))
    @test flocal(0.5) == 13

    # collect over the same range
    fcollect(st) = length(collect(0:st:6))
    @test fcollect(0.5) == 13

    # --- Regression guards: integer ranges keep the integer-typed fast path ---

    ri = [i for i in 1:5]
    @test ri == [1, 2, 3, 4, 5]
    @test eltype(ri) == Int64

    # integer range with an `Any` (function-parameter) BOUND must stay correct on
    # the integer path (guards against over-diverting `1:n`).
    fbound(n) = length([i for i in 1:n])
    @test fbound(5) == 5

    # integer stepped range keeps Int64 elements
    rs = [i for i in 1:2:10]
    @test rs == [1, 3, 5, 7, 9]
    @test eltype(rs) == Int64

    # --- Float32 range keeps a single float width (not widened to Float64) ---
    v32 = [u for u in 0:0.5f0:6]
    @test length(v32) == 13
    @test eltype(v32) == Float32

    # --- Multi-var cartesian and whitespace-flatten forms join types too ---
    mc = [u + v for u in 0:0.5:2, v in 1:2]
    @test size(mc) == (5, 2)
    @test sum(mc) == 25.0

    fl = [u for u in 0:0.5:2 for v in 1:2]
    @test length(fl) == 10
    @test sum(fl) == 10.0
end

true
