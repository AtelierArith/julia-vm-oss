# Issue #4816 (sibling to #4811): Vector{T}(::Vector{S}) where T != S
# returned the source vector unchanged instead of materializing a new
# Vector{T} with each element converted via T(x). Same compile-time
# intercept in compile_array_constructor as #4811, but for an Array
# argument instead of a Range.
#
# Fix: extended #4811's typed-comprehension synthesis to fire on Array
# args even when the source eltype already matches T, because upstream
# still allocates a fresh vector (Issue #10085).

using Test

@testset "Vector{Float64}(::Vector{Int}) — Int -> Float (Issue #4816)" begin
    v = Vector{Float64}([1, 2, 3])
    @test typeof(v) === Vector{Float64}
    @test eltype(v) === Float64
    @test v == [1.0, 2.0, 3.0]
end

@testset "Vector{Int64}(::Vector{Float}) — Float -> Int (Issue #4816)" begin
    v = Vector{Int64}([1.0, 2.0, 3.0])
    @test typeof(v) === Vector{Int64}
    @test eltype(v) === Int64
    @test v == [1, 2, 3]
end

@testset "Vector{Float32}(::Vector{Float64}) — Float64 -> Float32 (Issue #4816)" begin
    v = Vector{Float32}([1.0, 2.0, 3.0])
    @test typeof(v) === Vector{Float32}
    @test eltype(v) === Float32
    @test v == [1.0f0, 2.0f0, 3.0f0]
end

@testset "Array{Float64}(::Vector{Int}) — Array{T} alias (Issue #4816)" begin
    v = Array{Float64}([1, 2, 3])
    @test typeof(v) === Vector{Float64}
    @test v == [1.0, 2.0, 3.0]
end

@testset "Vector{T}(::Vector{T}) — same eltype copy path (Issues #4816, #10085)" begin
    src = [10, 20, 30]
    v = Vector{Int64}(src)
    @test typeof(v) === Vector{Int64}
    @test v == [10, 20, 30]
    @test !(v === src)
end

@testset "Vector{Float64}(::Vector{Float64}) — same Float eltype copy (Issues #4816, #10085)" begin
    src = [1.5, 2.5, 3.5]
    v = Vector{Float64}(src)
    @test typeof(v) === Vector{Float64}
    @test v == [1.5, 2.5, 3.5]
    @test !(v === src)
end

@testset "Vector{T}() empty regression (Issue #4816)" begin
    # Empty typed constructor stays on the empty-array path.
    v = Vector{Float64}()
    @test typeof(v) === Vector{Float64}
    @test length(v) == 0
end

@testset "Vector(arr) untyped regression (Issue #4816)" begin
    # No type args: direct constructor syntax still copies.
    src = [1, 2, 3]
    v = Vector(src)
    @test typeof(v) === Vector{Int64}
    @test v == [1, 2, 3]
    @test !(v === src)
end

true
