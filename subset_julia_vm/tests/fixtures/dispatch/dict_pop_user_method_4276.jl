using Test

import Base: pop!

pop!(d::Dict{String,Float64}, k::String) = :user_pop_4276
pop!(d::Dict{String,Float64}, k::String, default) = :user_pop_default_4276

dict_pop_any_4276(x::Any) = pop!(x, "a")
dict_pop_default_any_4276(x::Any) = pop!(x, "z", 9.0)

d = Dict("a" => 1.0)
@test pop!(d, "a") == :user_pop_4276

d = Dict("a" => 1.0)
@test dict_pop_any_4276(d) == :user_pop_4276

d = Dict("a" => 1.0)
@test pop!(d, "z", 9.0) == :user_pop_default_4276

d = Dict("a" => 1.0)
@test dict_pop_default_any_4276(d) == :user_pop_default_4276

d = Dict(:a => 1)
@test pop!(d, :a) == 1
@test length(d) == 0

d = Dict(:a => 1)
@test pop!(d, :z, 2) == 2
@test length(d) == 1

true
