module SparseArrays

using LinearAlgebra

export SparseMatrixCSC, sparse, spzeros

struct SparseMatrixCSC{Tv, Ti}
    m::Int
    n::Int
end

function sparse(args...; kwargs...)
    error("SparseArrays.sparse is not implemented in the sjulia compatibility package")
end

function spzeros(args...)
    sparse(args...)
end

end # module SparseArrays
