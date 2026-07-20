# Kept standalone: overrides Base methods on Base argument types, so the method
# table interaction is process-global and aggregation is order-dependent even
# under upstream julia (#5966 class; excluded from Issue #10238 module-wrap
# aggregation).
using Test

import Base: keys, values, pairs

keys(d::Dict{String,Float64}) = :user_keys_4276
values(d::Dict{String,Float64}) = :user_values_4276
pairs(d::Dict{String,Float64}) = :user_pairs_4276

dict_keys_any_4276(x::Any) = keys(x)
dict_values_any_4276(x::Any) = values(x)
dict_pairs_any_4276(x::Any) = pairs(x)

@testset "Dict view user methods before fallback (Issue #4276)" begin
    d = Dict("a" => 1.0)
    @test keys(d) == :user_keys_4276
    @test values(d) == :user_values_4276
    @test pairs(d) == :user_pairs_4276

    @test dict_keys_any_4276(d) == :user_keys_4276
    @test dict_values_any_4276(d) == :user_values_4276
    @test dict_pairs_any_4276(d) == :user_pairs_4276

    other = Dict(:a => 1)
    @test length(keys(other)) == 1
    @test length(values(other)) == 1
    @test length(pairs(other)) == 1
end

true
