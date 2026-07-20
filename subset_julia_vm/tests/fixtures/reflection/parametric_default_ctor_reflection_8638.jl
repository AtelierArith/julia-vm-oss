using Test

struct PW9_8638{T}
    x::T
    y
end

make9_8638(flag::Bool) = PW9_8638(flag ? 1.5 : 2.5, 41)
make9u_8638(flag) = PW9_8638(flag ? 1.5 : 2.5, 41)

@testset "parametric default constructor reflection return type" begin
    @test Base.infer_return_type(make9_8638, Tuple{Bool}) == PW9_8638{Float64}
    @test Base.infer_return_type(make9u_8638, Tuple{Bool}) == PW9_8638{Float64}

    typed_value = make9_8638(true)
    @test typed_value isa PW9_8638{Float64}
    @test typed_value.x == 1.5
    @test typed_value.y == 41

    @test Base.infer_return_type(make9_8638, Tuple{Bool}) == PW9_8638{Float64}
end

true
