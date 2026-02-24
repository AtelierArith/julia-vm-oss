# Val{:symbol} symbol specialization with @generated

using Test

function get_mode(::Val{mode}) where mode
    if @generated
        :(mode)
    else
        mode
    end
end

@testset "Val{Symbol} specialization" begin
    @test get_mode(Val{:fast}()) == :fast
    @test get_mode(Val{:safe}()) == :safe
end

true
