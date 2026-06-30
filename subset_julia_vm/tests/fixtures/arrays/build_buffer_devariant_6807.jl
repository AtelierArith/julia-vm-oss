# Issue #6807: the incremental array build buffer (NewArray*/PushElem*/Finalize*)
# is the last live VM producer of the legacy `Value::ExprArgs` carrier. It is
# emitted by the lazy specializer for typed array literals (`[1,2,3]`, etc.) and
# by the empty `Vector{String}` constants (ARGS/DEPOT_PATH/LOAD_PATH). This
# characterizes the value, element-type, ordering, mutation and special-layout
# (Complex / Tuple / struct) semantics of arrays produced through that build
# buffer so the de-variant onto the `Value::Memory` representation is provably
# behavior-preserving. Verified against upstream Julia 1.12.
using Test

struct Pt6807
    x::Int
    y::Int
end

# Force specialization by building the literals inside typed-arg functions.
make_int(a, b, c) = [a, b, c]
make_float(a, b) = [a, b]
make_bool(a, b) = [a, b]
make_str(a, b) = [a, b]
make_any() = [1, "two"]
make_complex(a, b) = [a, b]
make_tuple(a, b) = [a, b]
make_struct(a, b) = [a, b]

@testset "build buffer de-variant (Issue #6807)" begin
    # Int64 literal
    xi = make_int(10, 20, 30)
    @test xi == [10, 20, 30]
    @test eltype(xi) === Int64
    @test length(xi) == 3
    @test sum(xi) == 60
    push!(xi, 40)
    @test xi == [10, 20, 30, 40]

    # Float64 literal
    xf = make_float(1.5, 2.5)
    @test xf == [1.5, 2.5]
    @test eltype(xf) === Float64
    @test xf[2] === 2.5

    # Bool literal
    xb = make_bool(true, false)
    @test xb == [true, false]
    @test eltype(xb) === Bool
    @test count(xb) == 1

    # String literal
    xs = make_str("a", "b")
    @test xs == ["a", "b"]
    @test eltype(xs) === String
    @test xs[1] == "a"

    # Any (mixed) literal -> specializer's Any fallback
    xa = make_any()
    @test length(xa) == 2
    @test xa[1] === 1
    @test xa[2] == "two"
    @test eltype(xa) === Any

    # Complex literal -> boxed elements through the build buffer (value/ordering
    # parity; sjulia's specializer boxes complex literals as `Any` storage, so the
    # element *type* is intentionally not pinned here).
    xc = make_complex(1 + 2im, 3 + 4im)
    @test xc == [1 + 2im, 3 + 4im]
    @test real(xc[1]) == 1
    @test imag(xc[2]) == 4

    # Tuple-element literal -> AoS storage
    xt = make_tuple((1, 2), (3, 4))
    @test xt == [(1, 2), (3, 4)]
    @test xt[2] == (3, 4)
    @test length(xt) == 2

    # Struct-element literal -> heap struct refs
    xp = make_struct(Pt6807(1, 2), Pt6807(3, 4))
    @test length(xp) == 2
    @test xp[1].x == 1
    @test xp[2].y == 4

    # Empty typed array (the NewArrayTyped(_,0) + FinalizeArrayTyped path)
    empty_i = Int[]
    @test length(empty_i) == 0
    @test eltype(empty_i) === Int64
    push!(empty_i, 7)
    @test empty_i == [7]

    # ARGS is an empty Vector{String} built via the empty build-buffer path
    @test ARGS isa Vector{String}
    @test length(ARGS) == 0

    # 2-D literal goes through the build buffer with a rank-2 finalize shape
    m = [1 2; 3 4]
    @test size(m) == (2, 2)
    @test m[2, 1] == 3
    @test sum(m) == 10
end

true
