using Test

import Base: Pairs

const P = Pairs{Symbol,Int64,Tuple{Symbol},NamedTuple{(:a,),Tuple{Int64}}}

@test P <: AbstractDict
@test P <: AbstractDict{Symbol,Int64}
@test !(P <: AbstractDict{Symbol,Any})
@test !(P <: AbstractDict{Any,Int64})

true
