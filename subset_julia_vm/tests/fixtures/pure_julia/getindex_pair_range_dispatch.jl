# Regression: explicit `getindex(...)` calls on Pure Julia struct types
# (Pair, OneTo, LinRange, StepRangeLen, LogRange, CartesianIndex) reach
# Pure Julia method dispatch (Issue #3729).
# Direct calls used to be unconditionally routed to `compile_builtin_call`
# which never tries the method table.

using Test

@testset "explicit getindex on Pair (Issue #3729)" begin
    p = Pair(:a, 100)
    @test (getindex(p, 1) === :a)
    @test (getindex(p, 2) == 100)
    # Syntactic sugar still resolves to the same Pure Julia method.
    @test (p[1] === :a)
    @test (p[2] == 100)
end

@testset "explicit getindex on OneTo (Issue #3729)" begin
    r = Base.OneTo(5)
    @test (getindex(r, 1) == 1)
    @test (getindex(r, 3) == 3)
    @test (getindex(r, 5) == 5)
    @test (r[2] == 2)
end

@testset "explicit getindex on LinRange (Issue #3729)" begin
    r = LinRange(0.0, 10.0, 11)
    @test (getindex(r, 1) == 0.0)
    @test (getindex(r, 11) == 10.0)
    @test (r[6] == 5.0)
end

@testset "primitive Array indexing still works (Issue #3729)" begin
    a = [10, 20, 30]
    @test (getindex(a, 2) == 20)
    @test (a[1] == 10)
    setindex!(a, 999, 3)
    @test (a[3] == 999)
    a[1] = 555
    @test (a[1] == 555)
end

true
