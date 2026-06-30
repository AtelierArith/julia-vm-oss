using Test

import Base: empty!, merge!

empty!(d::Dict{String,Float64}) = :user_empty_4276
merge!(d::Dict{String,Float64}, other::Dict{String,Float64}) = :user_merge_4276

dict_empty_any_4276(x::Any) = empty!(x)
dict_merge_any_4276(x::Any, y::Any) = merge!(x, y)

d = Dict("a" => 1.0)
@test empty!(d) == :user_empty_4276

d = Dict("a" => 1.0)
@test dict_empty_any_4276(d) == :user_empty_4276

d = Dict(:a => 1)
emptied = empty!(d)
@test length(emptied) == 0
@test length(d) == 0

d = Dict("a" => 1.0)
other = Dict("b" => 2.0)
@test merge!(d, other) == :user_merge_4276

d = Dict("a" => 1.0)
other = Dict("b" => 2.0)
@test dict_merge_any_4276(d, other) == :user_merge_4276

d = Dict(:a => 1)
merged = merge!(d, Dict(:b => 2))
@test length(merged) == 2
@test length(d) == 2

true
