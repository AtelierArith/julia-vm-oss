# keytype / valtype for Dict / AbstractDict (Issue #5117)
# Both the type form `keytype(Dict{K,V}) === K` and the value form
# `keytype(d::AbstractDict) === K` must match upstream Julia exactly.

using Test

@testset "keytype/valtype on Dict types and instances (Issue #5117)" begin
    # --- Type form: extract K / V from the parametric Dict type ---
    @test keytype(Dict{String,Int}) === String
    @test valtype(Dict{String,Int}) === Int
    @test keytype(Dict{Int32,Float64}) === Int32
    @test valtype(Dict{Int32,Float64}) === Float64
    @test keytype(Dict{Symbol,Vector{Int}}) === Symbol
    @test valtype(Dict{Symbol,Vector{Int}}) === Vector{Int}

    # --- Value form: instance delegates through typeof ---
    @test keytype(Dict("a" => 1)) === String
    @test valtype(Dict("a" => 1)) === Int
    @test keytype(Dict(Int32(1) => "foo")) === Int32
    @test valtype(Dict(Int32(1) => "foo")) === String

    # --- Empty Dict() is Dict{Any,Any} ---
    @test keytype(Dict()) === Any
    @test valtype(Dict()) === Any

    # --- Explicitly parameterized empty instance ---
    d = Dict{Int,String}()
    @test keytype(d) === Int
    @test valtype(d) === String
end

true
