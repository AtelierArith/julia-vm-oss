# Issue #6627 / #6624: a faithful pure-Julia `Array{T,N}` over `Memory{T}` — the
# upstream storage shape (`ref`/`mem` + `size::NTuple{N,Int}`) — now compiles and
# round-trips end to end, on the foundations from #6623 (Memory{K} struct
# fields), #6625 (integer value type parameters: N as an Int), and #6626
# (MemoryRef{T} / Memory type values). This locks in that the Rust collection
# boundary can be just `Memory{T}`, with the container layered in pure Julia.
# Verified against upstream Julia 1.12.

using Test

# Upstream-shaped wrapper: a typed Memory plus an N-tuple of dimensions.
mutable struct MArray{T,N}
    mem::Memory{T}
    size::NTuple{N,Int}
end

marray(::Type{T}, n::Int) where {T} = MArray{T,1}(Memory{T}(undef, n), (n,))
function marray(::Type{T}, r::Int, c::Int) where {T}
    MArray{T,2}(Memory{T}(undef, r * c), (r, c))
end

Base.length(a::MArray) = a.size[1]
Base.ndims(::MArray{T,N}) where {T,N} = N
Base.size(a::MArray) = a.size

function Base.getindex(a::MArray{T,1}, i::Int) where {T}
    m = a.mem
    return m[i]
end
function Base.setindex!(a::MArray{T,1}, v, i::Int) where {T}
    m = a.mem
    m[i] = v
    return a
end

function vector_ok()
    a = marray(Int, 3)
    a[1] = 10
    a[2] = 20
    a[3] = 30
    s = 0
    for i in 1:length(a)
        s += a[i]
    end
    return typeof(a) == MArray{Int,1} &&
           length(a) == 3 &&
           ndims(a) == 1 &&
           a[2] == 20 &&
           s == 60
end

function matrix_shape_ok()
    a = marray(Float64, 2, 3)
    return typeof(a) == MArray{Float64,2} &&
           ndims(a) == 2 &&
           size(a) == (2, 3)
end

function string_elt_ok()
    a = marray(String, 2)
    a[1] = "x"
    a[2] = "y"
    return a[1] == "x" && a[2] == "y" && eltype(a.mem) == String
end

all_ok() = vector_ok() && matrix_shape_ok() && string_elt_ok()

@testset "pure-Julia Array{T,N} over Memory (#6627 / #6624)" begin
    @test vector_ok()
    @test matrix_shape_ok()
    @test string_elt_ok()
end

all_ok()
