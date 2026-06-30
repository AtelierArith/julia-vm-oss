module StaticArraysCore

export StaticArray, StaticVector, StaticMatrix, StaticVecOrMat, StaticScalar
export FieldVector, FieldMatrix
export SArray, SVector, SMatrix
export @SVector, @SMatrix, @SArray
export Size, Length, SOneTo, similar_type
export tuple_length, tuple_prod, tuple_minimum, size_to_tuple
export check_array_parameters, convert_ntuple
export StaticArrayStyle, Dynamic, StaticDimension

include("SOneTo.jl")
include("types.jl")
include("traits.jl")

end # module StaticArraysCore
