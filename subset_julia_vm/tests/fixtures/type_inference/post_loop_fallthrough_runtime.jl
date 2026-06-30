# Test: post-loop fallthrough runtime behaviour (Issue #3547).
# A loop body containing `return` may execute zero iterations, so the
# post-loop value must remain reachable. The function-return slot must
# accept the joined type — not the in-loop short-circuit type.
#
# Regression: previously the function's return slot was pinned to the
# in-loop `return i` type (I64). With n=0, the post-loop `"no iter"`
# string fell through and triggered a runtime "expected I64, got String"
# type error.

using Test

# Core repro: Int loop body return + String post-loop fallthrough.
function maybe_loop(n)
    for i in 1:n
        return i
    end
    "no iter"
end

# foreach variant.
function maybe_foreach(xs)
    for x in xs
        return x
    end
    "empty"
end

# while variant.
function maybe_while(go::Bool)
    while go
        return 42
    end
    "skipped"
end

# Nested: post-loop fallthrough is itself a string-typed expression.
function tail_string(n)
    for i in 1:n
        return i
    end
    s = "fall"
    s
end

@testset "Post-loop fallthrough runtime (Issue #3547)" begin
    @test maybe_loop(0) == "no iter"
    @test maybe_loop(3) == 1
    @test maybe_foreach(Int[]) == "empty"
    @test maybe_foreach([10, 20]) == 10
    @test maybe_while(false) == "skipped"
    @test maybe_while(true) == 42
    @test tail_string(0) == "fall"
    @test tail_string(2) == 1
end

true
