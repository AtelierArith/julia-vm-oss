module StaticArrays

using PrecompileTools
using LinearAlgebra  # extended with norm/normalize for StaticVector (Issue #8125)

export StaticArray, StaticVector, StaticMatrix, StaticVecOrMat, StaticScalar
export FieldVector, FieldMatrix
export SArray, SVector, SMatrix
export @SVector, @SMatrix, @SArray
export Size, Length, SOneTo, similar_type
export tuple_length, tuple_prod, tuple_minimum, size_to_tuple
export check_array_parameters, convert_ntuple
export StaticArrayStyle, Dynamic, StaticDimension

include("abstractarray.jl")
include("SArray.jl")
include("SVector.jl")
include("SMatrix.jl")
include("indexing.jl")
include("broadcast.jl")
include("arraymath.jl")
include("linalg.jl")
include("copy.jl")

end # module StaticArrays
