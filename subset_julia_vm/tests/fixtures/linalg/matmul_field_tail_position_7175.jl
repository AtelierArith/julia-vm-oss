# Issue #7175: a struct-field Matrix multiplied by a Vector inside a function
# body (tail position) must use matrix multiplication, not element-wise broadcast.
# Previously `dynamic_mul`'s Array*Array path did element-wise `.*`, so `b.W * w`
# in tail position silently returned `W .* w`.
using Test

struct Box
    W::Matrix{Float64}
end

# field access in tail position — the exact failing shape
ffield(b::Box, w) = b.W * w

# callable struct (functor) reading its own fields
struct Affine
    W::Matrix{Float64}
    b::Vector{Float64}
end
(a::Affine)(x) = a.W * x + a.b

@testset "Issue #7175: field matrix * vector in tail position is matmul" begin
    box = Box([1.0 2.0; 3.0 4.0])
    v = [1.0, 1.0]

    # tail-position field matmul
    @test ffield(box, v) == [3.0, 7.0]

    # non-tail still correct (was already fine)
    @test (box.W * v) == [3.0, 7.0]

    # functor with field access
    f = Affine([0.85 0.04; -0.04 0.85], [0.0, 1.6])
    r = f([1.0, 2.0])
    @test r[1] ≈ 0.93
    @test r[2] ≈ 3.26

    # `*` is matmul, `.*` stays element-wise — they must differ
    A = [1.0 2.0; 3.0 4.0]
    B = [10.0 20.0; 30.0 40.0]
    @test (A * B) == [70.0 100.0; 150.0 220.0]
    @test (A .* B) == [10.0 40.0; 90.0 160.0]
end

true
