using Test

complex_array_dispatch_3908(a::Vector{Complex{Float64}}) = :complex_vector
complex_array_dispatch_3908(a::Vector{Float64}) = :float_vector
complex_array_dispatch_3908(a) = :fallback

complex_array_dispatch_any_3908(a::Any) = complex_array_dispatch_3908(a)

@testset "complex array dispatch uses logical element type (Issue #3908)" begin
    zeros_complex = zeros(Complex{Float64}, 2)
    ones_complex = ones(Complex{Float64}, 2)
    erased = Any[zeros_complex]

    @test complex_array_dispatch_3908(zeros_complex) == :complex_vector
    @test complex_array_dispatch_any_3908(ones_complex) == :complex_vector
    @test complex_array_dispatch_3908(erased[1]) == :complex_vector
    @test complex_array_dispatch_3908([1.0, 2.0]) == :float_vector
end

true
