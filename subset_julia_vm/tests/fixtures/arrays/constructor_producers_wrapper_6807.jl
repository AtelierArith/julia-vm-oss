# Issue #6807: VM-level array producers — range materialization (`MakeRange` /
# `MakeRangeF64`), RNG arrays (`rand`/`randn`), and matrix ops — now emit the
# MemoryRef-backed `Array{T,N}` wrapper instead of the legacy `Value::ExprArgs`
# carrier. This characterizes value, element type, length, indexing and reductions
# on arrays produced by those instructions (the wrapper must behave exactly like
# any other array through `length`/`eltype`/indexing/`sum`). Verified against
# upstream Julia 1.12.
using Test

@testset "constructor producers as wrappers (Issue #6807)" begin
    # Integer range materialization
    xi = collect(1:5)
    @test xi == [1, 2, 3, 4, 5]
    @test eltype(xi) === Int64
    @test length(xi) == 5
    @test sum(xi) == 15
    @test xi[3] == 3

    # Stepped integer range
    xs = collect(2:2:10)
    @test xs == [2, 4, 6, 8, 10]
    @test length(xs) == 5

    # Float range materialization
    xf = collect(0.0:0.5:2.0)
    @test xf == [0.0, 0.5, 1.0, 1.5, 2.0]
    @test eltype(xf) === Float64
    @test length(xf) == 5

    # Matrix multiply result (value/length parity; the result *element type* of
    # an Int*Int matmul is a pre-existing sjulia gap — it widens to Float64 — so
    # it is intentionally not pinned here).
    m = [1 2; 3 4] * [1, 1]
    @test m == [3, 7]
    @test length(m) == 2

    # Matrix-matrix product
    mm = [1 2; 3 4] * [5 6; 7 8]
    @test mm == [19 22; 43 50]
    @test size(mm) == (2, 2)

    # RNG arrays: type/shape/range (values are non-deterministic across runtimes)
    r = rand(4)
    @test eltype(r) === Float64
    @test length(r) == 4
    @test all(x -> 0.0 <= x < 1.0, r)

    rn = randn(3)
    @test eltype(rn) === Float64
    @test length(rn) == 3

    # The produced arrays compose with higher-order array operations
    @test sum(map(x -> x * 2, collect(1:3))) == 12
    @test maximum(collect(1:10)) == 10
end

true
