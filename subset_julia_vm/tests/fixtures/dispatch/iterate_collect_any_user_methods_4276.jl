using Test

import Base: iterate, collect

iterate(xs::Vector{Int64}) = (:user_iterate_4276, 1)
collect(xs::Vector{Int64}) = [:user_collect_4276]

runtime_iterate_any_4276(xs::Any) = iterate(xs)
runtime_collect_any_4276(xs::Any) = collect(xs)

xs = [1, 2, 3]

@testset "Any-typed iterate/collect use user Vector methods" begin
    @test iterate(xs) == (:user_iterate_4276, 1)
    @test runtime_iterate_any_4276(xs) == (:user_iterate_4276, 1)

    @test collect(xs) == [:user_collect_4276]
    @test runtime_collect_any_4276(xs) == [:user_collect_4276]
end

# Gate the manifest `expected = true` on the actual dispatch results, not a
# bare `true`. The `runtime_iterate_any_4276` value regression (Issue #6638)
# was previously masked because the @testset failure never propagated to
# nextest's `dispatch::` value check (Issue #4276 test-quality note).
iterate(xs) == (:user_iterate_4276, 1) &&
    runtime_iterate_any_4276(xs) == (:user_iterate_4276, 1) &&
    collect(xs) == [:user_collect_4276] &&
    runtime_collect_any_4276(xs) == [:user_collect_4276]
