# Test array return type annotations (::Array{T,N})

using Test

# Vector return type (1D array)
function make_vector(n::Int64)::Array{Int64,1}
    result = zeros(Int64, n)
    for i in 1:n
        result[i] = i * i
    end
    return result
end

# Matrix return type (2D array)
function make_matrix(rows::Int64, cols::Int64)::Array{Int64,2}
    result = zeros(Int64, rows, cols)
    for i in 1:rows
        for j in 1:cols
            result[i, j] = i + j
        end
    end
    return result
end

# Alternative syntax: Vector{T} and Matrix{T}
function make_vector_alt(n::Int64)::Vector{Float64}
    result = zeros(Float64, n)
    for i in 1:n
        result[i] = Float64(i) * 0.5
    end
    return result
end

function make_matrix_alt(n::Int64)::Matrix{Float64}
    result = zeros(Float64, n, n)
    for i in 1:n
        result[i, i] = 1.0
    end
    return result
end

@testset "Array return type annotations" begin
    v = make_vector(3)
    @test length(v) == 3
    @test v[1] == 1
    @test v[2] == 4
    @test v[3] == 9

    m = make_matrix(2, 3)
    @test size(m) == (2, 3)
    @test m[1, 1] == 2
    @test m[2, 3] == 5

    v2 = make_vector_alt(4)
    @test length(v2) == 4
    @test v2[2] == 1.0

    m2 = make_matrix_alt(3)
    @test m2[1, 1] == 1.0
    @test m2[2, 2] == 1.0
end

true
