# Test type-preserving array allocation idioms inside function bodies
# (Issue #3648). Each of `similar(arr, n)`, `collect(arr)`, and
# `Vector{eltype(arr)}(undef, n)` must preserve the element type when
# called from inside a function with an Any-typed parameter.

using Test

@testset "similar(arr, n) preserves element type from inside a function" begin
    f(arr) = similar(arr, 2)
    @test typeof(f([1, 2, 3])) == Vector{Int64}
    @test typeof(f([1.0, 2.0, 3.0])) == Vector{Float64}
    @test typeof(f([true, false])) == Vector{Bool}
end

@testset "similar(arr) (no length) preserves element type from inside a function" begin
    g(arr) = similar(arr)
    @test typeof(g([1, 2, 3])) == Vector{Int64}
    @test typeof(g([1.0, 2.0])) == Vector{Float64}
    @test typeof(g([true, false])) == Vector{Bool}
end

@testset "collect(arr) preserves element type from inside a function" begin
    function rev(arr)
        n = length(arr)
        result = collect(arr)
        for i in 1:n
            result[i] = arr[n - i + 1]
        end
        return result
    end
    @test typeof(rev([1, 2, 3])) == Vector{Int64}
    @test rev([1, 2, 3]) == [3, 2, 1]
    @test typeof(rev([1.0, 2.0, 3.0])) == Vector{Float64}
    @test rev([1.0, 2.0, 3.0]) == [3.0, 2.0, 1.0]
    @test typeof(rev([true, false, true])) == Vector{Bool}
end

@testset "collect(arr) returns an independent shape-preserving copy" begin
    src = [1, 2, 3]
    dst = collect(src)
    dst[1] = 99
    @test src == [1, 2, 3]
    @test dst == [99, 2, 3]
    @test typeof(dst) == Vector{Int64}

    empty = collect(Int64[])
    @test typeof(empty) == Vector{Int64}
    @test length(empty) == 0

    mat = [1 2; 3 4]
    mat_copy = collect(mat)
    @test typeof(mat_copy) == Matrix{Int64}
    @test size(mat_copy) == (2, 2)
    @test mat_copy == mat
    mat_copy[1, 2] = 20
    @test mat == [1 2; 3 4]
    @test mat_copy == [1 20; 3 4]

    bools = [true false; false true]
    bool_copy = collect(bools)
    @test typeof(bool_copy) == Matrix{Bool}
    @test size(bool_copy) == (2, 2)
    @test bool_copy == bools
end

@testset "Vector{eltype(arr)}(undef, n) preserves element type" begin
    function f(arr)
        T = eltype(arr)
        result = Vector{T}(undef, 2)
        result[1] = arr[1]
        result[2] = arr[2]
        return result
    end
    @test typeof(f([1, 2, 3])) == Vector{Int64}
    @test f([1, 2, 3]) == [1, 2]
    @test typeof(f([1.0, 2.0, 3.0])) == Vector{Float64}
    @test typeof(f([true, false, true])) == Vector{Bool}
end

@testset "Vector{T}(undef, n) inside where T function" begin
    function fwhere(arr::Vector{T}) where T
        return Vector{T}(undef, 2)
    end
    @test typeof(fwhere([1, 2, 3])) == Vector{Int64}
    @test typeof(fwhere([1.0, 2.0, 3.0])) == Vector{Float64}
    @test typeof(fwhere([true, false])) == Vector{Bool}
end

@testset "reverse([1, 2, 3]) is type-preserving (#3648)" begin
    @test typeof(reverse([1, 2, 3])) == Vector{Int64}
    @test reverse([1, 2, 3]) == [3, 2, 1]
    @test typeof(reverse([1.0, 2.0])) == Vector{Float64}
    @test typeof(reverse([true, false])) == Vector{Bool}
end

@testset "broadcast still works (regression check for similar dispatch)" begin
    x = [1, 2, 3]
    y = x .+ 1
    @test typeof(y) == Vector{Int64}
    @test y == [2, 3, 4]

    a = [1.0, 2.0, 3.0]
    b = a .* 2
    @test typeof(b) == Vector{Float64}
end

true
