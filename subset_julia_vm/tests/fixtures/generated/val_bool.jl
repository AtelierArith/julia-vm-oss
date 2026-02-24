# Val{true}/Val{false} boolean specialization with @generated

using Test

function check_debug(::Val{debug}) where debug
    if @generated
        :(debug)
    else
        debug
    end
end

@testset "Val{Bool} specialization" begin
    @test check_debug(Val{true}()) == true
    @test check_debug(Val{false}()) == false
end

true
