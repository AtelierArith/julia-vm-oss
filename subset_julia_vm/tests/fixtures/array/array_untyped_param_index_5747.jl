# Issue #5747: `a[k]` where `k` is an UNTYPED parameter that holds a Range or
# Vector at runtime must index by whatever `k` is — a sub-array, not a scalar
# element. Previously the compiler treated an untyped-parameter index as scalar
# (it only recognized a LITERAL range/`:` as a slice), so it emitted a scalar
# `IndexLoad` and inferred the array element type (Int64); the genuine sub-array
# then hit `expected I64, got Range` (the load) or `ReturnI64`/`StoreI64`
# (the return/binding coercion).

using Test

@testset "untyped-param index a[k] with runtime Range/Vector (Issue #5747)" begin
    f(a, k) = a[k]

    # Range index -> sub-array
    @test f([10, 20, 30, 40], 2:3) == [20, 30]
    @test f([10, 20, 30, 40], 1:2:4) == [10, 30]

    # Vector index -> sub-array
    @test f([10, 20, 30, 40], [1, 3]) == [10, 30]

    # scalar index -> element (regression: must stay scalar)
    @test f([10, 20, 30, 40], 2) == 20

    # the sub-array result is a real array: bind it, index it, reduce it
    r = f([10, 20, 30, 40], 2:3)
    @test r == [20, 30]
    @test r[1] == 20
    @test length(f([10, 20, 30, 40], 2:3)) == 2
    @test sum(f([10, 20, 30, 40], 1:4)) == 100

    # Float array stays Float
    @test f([1.0, 2.0, 3.0], 2:3) == [2.0, 3.0]

    # typed-param forms still work (regression)
    g(a, k::AbstractRange) = a[k]
    @test g([10, 20, 30, 40], 2:3) == [20, 30]
end

true
