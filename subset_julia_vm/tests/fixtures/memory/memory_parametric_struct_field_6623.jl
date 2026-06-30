# Issue #6623: Memory{K} / Memory{V} used as PARAMETRIC struct fields must
# compile and bind the struct's type parameters correctly. Previously the field
# type `Memory{K}` fell through the struct field-type resolver to the F64
# "unknown base type" default, so constructing the struct raised
# "Cannot convert MemoryOf(...) to F64". This is the foundation for re-basing
# Dict/Array on Memory{T} (the Rust boundary). Verified against upstream Julia.

using Test

struct Pair2{K,V}
    ks::Memory{K}
    vs::Memory{V}
end

mutable struct Box1{T}
    mem::Memory{T}
end

function build_pair()
    ks = Memory{String}(undef, 2)
    vs = Memory{Int}(undef, 2)
    ks[1] = "a"
    ks[2] = "b"
    vs[1] = 10
    vs[2] = 20
    return Pair2{String,Int}(ks, vs)
end

function types_ok()
    p = build_pair()
    # type parameters bind from the explicit type args, not the field values
    a1 = typeof(p) == Pair2{String,Int}
    # fields are the typed Memory values, accessed via locals
    _ks = p.ks
    _vs = p.vs
    a2 = _ks[1] == "a" && _ks[2] == "b"
    a3 = _vs[1] == 10 && _vs[2] == 20
    a4 = eltype(_ks) == String && eltype(_vs) == Int
    return a1 && a2 && a3 && a4
end

function box_ok()
    m = Memory{Float64}(undef, 3)
    m[1] = 1.5
    b = Box1{Float64}(m)
    _m = b.mem
    return typeof(b) == Box1{Float64} && _m[1] == 1.5 && eltype(_m) == Float64
end

all_ok() = types_ok() && box_ok()

@testset "Memory{K} as a parametric struct field (#6623)" begin
    @test types_ok()
    @test box_ok()
end

all_ok()
