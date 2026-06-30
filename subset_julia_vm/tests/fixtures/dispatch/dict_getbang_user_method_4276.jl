using Test

import Base: get!

get!(d::Dict{String,Float64}, k::String, default) = :user_getbang_4276

dict_getbang_any_4276(x::Any) = get!(x, "b", 2.0)

d = Dict("a" => 1.0)
@test get!(d, "b", 2.0) == :user_getbang_4276

d = Dict("a" => 1.0)
@test dict_getbang_any_4276(d) == :user_getbang_4276

@test get!(Dict(:a => 1), :b, 2) == 2

true
