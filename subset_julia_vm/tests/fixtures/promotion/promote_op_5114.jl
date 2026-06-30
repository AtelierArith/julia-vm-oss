# Base.promote_op (operation result type) — Issue #5114
#
# promote_op(f, Ts...) returns an upper bound on the type of f(xs...) where
# xs::Ts, inferred from the argument TYPES alone. Verified against upstream
# Julia 1.12:
#   Base.promote_op(+, Int, Float64) === Float64
#   Base.promote_op(*, Int, Int)     === Int64
#   Base.promote_op(/, Int, Int)     === Float64
#   Base.promote_op(-, Int, Int)     === Int64

using Test

# A user function for the inference-driven (non-operator) path.
addone(x) = x + 1

@testset "promote_op (Issue #5114)" begin
    # Elementary arithmetic operators over concrete numeric arguments
    @test Base.promote_op(+, Int, Float64) === Float64
    @test Base.promote_op(*, Int, Int) === Int64
    @test Base.promote_op(+, Int, Int) === Int64
    @test Base.promote_op(-, Int, Int) === Int64
    @test Base.promote_op(/, Int, Int) === Float64
    @test Base.promote_op(*, Float64, Float64) === Float64
    @test Base.promote_op(+, Int32, Int64) === Int64

    # User-defined function: inferred from the tuple signature
    @test Base.promote_op(addone, Int) === Int64
end

true
