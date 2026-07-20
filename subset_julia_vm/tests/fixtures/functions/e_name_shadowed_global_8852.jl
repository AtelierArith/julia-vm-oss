using Test

e(x::Int) = x + 1
e(x::Float64) = x + 0.5
call_e_8852(x) = e(x)

@testset "function named e shadows MathConstants global type" begin
    @test e(1) == 2
    @test call_e_8852(2) == 3
    @test e(1.5) == 2.0
end

true
