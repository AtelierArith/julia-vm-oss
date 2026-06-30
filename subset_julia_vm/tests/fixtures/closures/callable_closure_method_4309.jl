using Test

f4309 = x -> x + 1
T4309 = typeof(f4309)

(::T4309)(x::String) = x * "!"

function closure_method_call_4309()
    f4309("a")
end

@testset "callable closure method #4309" begin
    @test f4309(1) == 2
    @test f4309("a") == "a!"
    @test closure_method_call_4309() == "a!"
end

true
