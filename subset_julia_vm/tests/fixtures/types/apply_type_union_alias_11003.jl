# Core.apply_type with const-Union-alias bounds (Issue #11003)
#
# The runtime bound checker resolves a `const` alias to a Union when
# validating a parametric type's upper bound.

using Test

const Elem109 = Union{Integer, String}

struct Box109{T<:Elem109}
    x::T
    function Box109{T}(x) where {T<:Elem109}
        new{T}(x)
    end
end

@testset "apply_type through const Union alias bounds" begin
    @test Core.apply_type(Box109, Int)(1).x == 1
    @test Core.apply_type(Box109, String)("s").x == "s"
    @test Box109{BigInt}(big(3)).x == big(3)
    @test_throws TypeError Core.apply_type(Box109, Float64)
end

true
