# Kept standalone: overrides Base methods on Base argument types, so the method
# table interaction is process-global and aggregation is order-dependent even
# under upstream julia (#5966 class; excluded from Issue #10238 module-wrap
# aggregation).
using Test

import Base: keytype, valtype

keytype(d::Dict{String,Float64}) = :user_keytype_4276
valtype(d::Dict{String,Float64}) = :user_valtype_4276

dict_keytype_any_4276(x::Any) = keytype(x)
dict_valtype_any_4276(x::Any) = valtype(x)

d = Dict("a" => 1.0)
@test keytype(d) == :user_keytype_4276
@test valtype(d) == :user_valtype_4276

d = Dict("a" => 1.0)
@test dict_keytype_any_4276(d) == :user_keytype_4276
@test dict_valtype_any_4276(d) == :user_valtype_4276

@test keytype(Dict(:a => 1)) == Symbol
@test valtype(Dict(:a => 1)) == Int64

true
