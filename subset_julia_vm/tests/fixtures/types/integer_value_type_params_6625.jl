# Issue #6625: integer value type parameters work — a parametric struct can take
# an Int type parameter `N`, hold an `NTuple{N,Int}` field, and recover `N` via
# `where {T,N}` dispatch. This is the exact upstream `Array{T,N}` shape
# (`mutable struct Array{T,N}; ref::MemoryRef{T}; size::NTuple{N,Int}; end`) and
# is the type-system foundation for re-basing Array/Vector on Memory (#6624).
# Verified against upstream Julia 1.12.

using Test

# The upstream Array{T,N} struct shape, built over a typed Memory{T} (works after
# the #6623 Memory{K}-field fix).
mutable struct Arr2{T,N}
    ref::Memory{T}
    size::NTuple{N,Int}
end

ndims_of(::Arr2{T,N}) where {T,N} = N
eltype_of(::Arr2{T,N}) where {T,N} = T

function shape_ok()
    m = Memory{Int}(undef, 6)
    a = Arr2{Int,2}(m, (2, 3))
    return typeof(a) == Arr2{Int,2} &&
           a.size == (2, 3) &&
           ndims_of(a) == 2 &&
           eltype_of(a) == Int
end

# N dispatch selects different methods for different N.
kind(::Arr2{T,1}) where {T} = "vector"
kind(::Arr2{T,2}) where {T} = "matrix"

function dispatch_on_n_ok()
    v = Arr2{Float64,1}(Memory{Float64}(undef, 3), (3,))
    m = Arr2{Float64,2}(Memory{Float64}(undef, 6), (2, 3))
    return kind(v) == "vector" && kind(m) == "matrix"
end

# NTuple{N,Int} and Array{T,N} aliases on the built-in side.
function ntuple_and_alias_ok()
    t = ntuple(i -> i * i, 3)
    return typeof(t) == NTuple{3,Int} &&
           Array{Int,2} === Matrix{Int} &&
           Array{Float64,1} === Vector{Float64}
end

all_ok() = shape_ok() && dispatch_on_n_ok() && ntuple_and_alias_ok()

@testset "integer value type parameters / Array{T,N} shape (#6625)" begin
    @test shape_ok()
    @test dispatch_on_n_ok()
    @test ntuple_and_alias_ok()
end

all_ok()
