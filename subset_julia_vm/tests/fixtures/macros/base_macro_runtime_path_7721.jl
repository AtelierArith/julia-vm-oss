using Test
using Printf

@testset "Base registry macros expand through runtime path" begin
    @test @sprintf("%d-%d", 1, 2) == "1-2"
    @test @something(nothing, 5) == 5
    @test @coalesce(missing, 6) == 6

    shown = @show 2 + 3
    @test shown == 5

    @assert true
    @assert string([1 2; 3 4]) == "[1 2; 3 4]"
    @assert DimensionMismatch <: Exception
    @assert typeof(170141183460469231731687303715884105728) == BigInt
    @assert eval(Meta.parse("Val(3)")) == Val{3}()

    function describe(args...; kwargs...)
        length(args) + length(kwargs)
    end
    pos = (1, 2, 3)
    opts = (a = 1, b = 2)
    f = describe
    @assert f(pos...; opts...) == 5

    @info "runtime macro" value=42
end

true
