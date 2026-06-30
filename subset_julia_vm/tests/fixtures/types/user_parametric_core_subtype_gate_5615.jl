using Test

abstract type AnimalParam5615 end
abstract type VehicleParam5615 end

struct BoxParam5615{T} <: AnimalParam5615
    value::T
end

struct CrateParam5615{T} <: VehicleParam5615
    value::T
end

@test BoxParam5615{Int64} <: AnimalParam5615
@test BoxParam5615{String} <: AnimalParam5615
@test !(BoxParam5615{Int64} <: VehicleParam5615)
@test !(CrateParam5615{Int64} <: AnimalParam5615)

@test Tuple{BoxParam5615{Int64}} <: Tuple{AnimalParam5615}
@test Tuple{Tuple{BoxParam5615{String}}, Int64} <: Tuple{Tuple{AnimalParam5615}, Real}
@test !(Tuple{BoxParam5615{Int64}} <: Tuple{VehicleParam5615})
@test !(Tuple{CrateParam5615{Int64}} <: Tuple{AnimalParam5615})

true
