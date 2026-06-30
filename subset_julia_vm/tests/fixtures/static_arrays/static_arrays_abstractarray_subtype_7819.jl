using StaticArrays

# Issue #7819 / #7728: StaticArray must subtype AbstractArray{T,N} (upstream
# spelling), preserving the concrete element type and rank through the
# value-parameter intermediate chain
# SVector{N,T} <: StaticVector{N,T} <: StaticVecOrMat{Tuple{N},T,1}
#            <: StaticArray{Tuple{N},T,1} <: AbstractArray{T,N}.
v = SVector{3, Int64}(1, 2, 3)

ok = v isa AbstractArray{Int64, 1} &&
     v isa AbstractArray{Int64} &&
     v isa AbstractArray &&
     !(v isa AbstractArray{Float64, 1}) &&
     SVector{3, Int64} <: AbstractArray{Int64, 1} &&
     SVector{3, Int64} <: AbstractArray &&
     !(SVector{3, Int64} <: AbstractArray{Float64, 1})

println(ok)
ok
