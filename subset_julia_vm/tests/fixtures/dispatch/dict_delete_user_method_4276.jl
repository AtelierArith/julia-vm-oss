using Test

import Base: delete!

delete!(d::Dict{String,Float64}, k::String) = :user_delete_4276

dict_delete_any_4276(x::Any) = delete!(x, "a")

d = Dict("a" => 1.0)
@test delete!(d, "a") == :user_delete_4276

d = Dict("a" => 1.0)
@test dict_delete_any_4276(d) == :user_delete_4276

d = Dict(:a => 1)
deleted = delete!(d, :a)
@test length(deleted) == 0
@test length(d) == 0

true
