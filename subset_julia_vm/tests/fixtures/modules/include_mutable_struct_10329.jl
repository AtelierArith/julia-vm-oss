using Test

definition_path10329 = joinpath(@__DIR__, "include_mutable_struct_10329_def.jl")
result10329 = include(definition_path10329)

@test result10329 === true

true
