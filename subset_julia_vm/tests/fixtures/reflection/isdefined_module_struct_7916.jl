# isdefined(::Module, ::Symbol) recognizes struct/type bindings defined
# *inside* a module (Issue #7916).
#
# Types declared in `module M ... end` are registered under a module-qualified
# name internally, so the binding check must match that qualified form (in
# addition to the unqualified name) — otherwise `isdefined(M, :Box)` returned
# `false` even though `M.Box(3)` constructs fine.
using Test

module M
mutable struct Box
    x::Int
end

struct Pair2
    a::Int
    b::Int
end

abstract type Animal end
end

@testset "isdefined finds module-scoped struct bindings (#7916)" begin
    @test isdefined(M, :Box)
    @test isdefined(M, :Pair2)
    @test isdefined(M, :Animal)
    @test isdefined(M, :NotAType) == false
end

@testset "module-scoped struct still constructs and reads fields" begin
    @test M.Box(3).x == 3
    @test M.Pair2(1, 2).b == 2
end

true
