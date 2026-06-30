# Issue #4811: Vector{T}(::AbstractRange) (typed constructor) returned the
# range unchanged because the compile-time intercept at
# `compile_array_constructor` only routed the no-type-args case
# (`Vector(range)`) through `RangeCollect`. The typed case fell through
# to `self.compile_expr(&args[0])`, which is a no-op for the range.
#
# Fix: in the typed 1-arg range case, synthesize a typed comprehension
# `T[T(x) for x in r]` and compile that. The existing
# typed-comprehension compile path materializes and per-element
# converts, matching upstream Julia's `Base.Vector{T}(r::AbstractRange)
# = T[x for x in r]` (the constructor call form `T(x)` is the
# conversion).

using Test

@testset "Vector{Float64}(::UnitRange{Int}) (Issue #4811)" begin
    v = Vector{Float64}(1:3)
    @test typeof(v) === Vector{Float64}
    @test v == [1.0, 2.0, 3.0]
end

@testset "Vector{Int64}(::StepRangeLen Float) (Issue #4811)" begin
    v = Vector{Int64}(1.0:3.0)
    @test typeof(v) === Vector{Int64}
    @test v == [1, 2, 3]
end

@testset "Vector{Float32}(::UnitRange{Int}) (Issue #4811)" begin
    v = Vector{Float32}(1:3)
    @test typeof(v) === Vector{Float32}
    @test v == [1.0f0, 2.0f0, 3.0f0]
end

@testset "Vector{Float64}(::StepRange{Int}) (Issue #4811)" begin
    v = Vector{Float64}(1:2:9)
    @test typeof(v) === Vector{Float64}
    @test v == [1.0, 3.0, 5.0, 7.0, 9.0]
end

@testset "Vector{Float64}(::StepRangeLen Float step) (Issue #4811)" begin
    v = Vector{Float64}(1.0:0.5:3.0)
    @test typeof(v) === Vector{Float64}
    @test v == [1.0, 1.5, 2.0, 2.5, 3.0]
end

@testset "Array{Float64}(::UnitRange{Int}) (Issue #4811)" begin
    v = Array{Float64}(1:3)
    @test typeof(v) === Vector{Float64}
    @test v == [1.0, 2.0, 3.0]
end

@testset "Vector(::AbstractRange) regression — no eltype (Issue #4810)" begin
    # Confirms the no-type-args path (Issue #4810) still works.
    v = Vector(1:3)
    @test typeof(v) === Vector{Int64}
    @test v == [1, 2, 3]
end

@testset "Vector{T}(::Array) regression — non-range arg (Issue #4811)" begin
    # Confirms my typed-range intercept did not regress the
    # non-range case (the no-op shallow copy is preserved).
    src = [10, 20, 30]
    v = Vector{Int64}(src)
    @test typeof(v) === Vector{Int64}
    @test v == [10, 20, 30]
end

true
