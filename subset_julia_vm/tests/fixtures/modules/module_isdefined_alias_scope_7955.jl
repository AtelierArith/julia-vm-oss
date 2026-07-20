using Test

module AliasScope7955A

export T

const T = Int64

struct Box{T}
    value::T
end

abstract type AbstractThing end

end

module AliasScope7955B
end

using .AliasScope7955A

@testset "isdefined module alias scope (PR #7955)" begin
    @test T === Int64
    @test T(1) == 1
    @test typeof(T(1)) === Int64
    @test AliasScope7955A.T(2) == 2
    @test typeof(AliasScope7955A.T(2)) === Int64
    @test isdefined(AliasScope7955A, :T)
    @test !isdefined(AliasScope7955B, :T)

    @test isdefined(AliasScope7955A, :Box)
    @test !isdefined(AliasScope7955B, :Box)

    @test isdefined(AliasScope7955A, :AbstractThing)
    @test !isdefined(AliasScope7955B, :AbstractThing)
end

true
