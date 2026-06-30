using Test

@testset "adjoint preserves Complex{Float64} element type (#4018, #4604)" begin
    v = zeros(Complex{Float64}, 2)
    v[1] = 1 + 2im
    v[2] = 3 - 4im
    row = adjoint(v)
    @test eltype(row) == Complex{Float64}
    @test size(row) == (1, 2)
    @test typeof(row[1, 1]) == Complex{Float64}
    @test typeof(row[1, 2]) == Complex{Float64}
    @test row[1, 1] == 1 - 2im
    @test row[1, 2] == 3 + 4im

    A = zeros(Complex{Float64}, 2, 2)
    A[1, 1] = 1 + 2im
    A[2, 1] = 3 + 4im
    A[1, 2] = 5 - 6im
    A[2, 2] = 7 - 8im
    transposed = adjoint(A)
    @test eltype(transposed) == Complex{Float64}
    @test size(transposed) == (2, 2)
    @test typeof(transposed[1, 1]) == Complex{Float64}
    @test typeof(transposed[1, 2]) == Complex{Float64}
    @test typeof(transposed[2, 1]) == Complex{Float64}
    @test typeof(transposed[2, 2]) == Complex{Float64}
    @test transposed[1, 1] == 1 - 2im
    @test transposed[1, 2] == 3 - 4im
    @test transposed[2, 1] == 5 + 6im
    @test transposed[2, 2] == 7 + 8im
end

true
