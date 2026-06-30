# Issue #5162: `<Type>.parameters` includes integer/value parameters, not just
# type parameters. Upstream Julia: `Array{T,N}.parameters == svec(T, N)`,
# `Vector{Int}.parameters == svec(Int64, 1)`, `Val{5}.parameters == svec(5)`.
# Follow-up to #4722/#5161 (svec identity for `.parameters`).
# Verified against upstream Julia 1.12 (parity).

using Test

@testset "value/integer parameters in <Type>.parameters (Issue #5162)" begin
    # --- Array dimensionality value parameter N -------------------------------
    @test Vector{Int}.parameters == Core.svec(Int64, 1)
    @test Matrix{Float64}.parameters == Core.svec(Float64, 2)
    @test Array{Int,3}.parameters == Core.svec(Int64, 3)
    @test typeof([1]).parameters == Core.svec(Int64, 1)
    @test typeof([1.0 2.0; 3.0 4.0]).parameters == Core.svec(Float64, 2)

    # The value parameter is the integer itself (an Int64), not a type.
    @test Vector{Int}.parameters[2] === 1
    @test Matrix{Float64}.parameters[2] === 2
    @test Array{Int,3}.parameters[2] === 3
    @test typeof(Vector{Int}.parameters[2]) === Int64

    # Type parameter still comes first.
    @test Vector{Int}.parameters[1] === Int64
    @test Matrix{Float64}.parameters[1] === Float64

    # Length now counts the value parameter.
    @test length(Vector{Int}.parameters) == 2
    @test length(Matrix{Float64}.parameters) == 2
    @test length(Array{Int,3}.parameters) == 2

    # --- Val: a pure value parameter ------------------------------------------
    @test Val{5}.parameters == Core.svec(5)
    @test Val{:foo}.parameters == Core.svec(:foo)
    @test Val{true}.parameters == Core.svec(true)
    @test typeof(Val(7)).parameters == Core.svec(7)
    @test Val{5}.parameters[1] === 5
    @test typeof(Val{5}.parameters[1]) === Int64
    @test Val{:foo}.parameters[1] === :foo
    @test Val{true}.parameters[1] === true
    @test length(Val{5}.parameters) == 1

    # --- Type-only parameters unaffected (no regression of #5161 svec) --------
    @test Tuple{Int,String}.parameters == Core.svec(Int64, String)
    @test NTuple{3,Int}.parameters == Core.svec(Int64, Int64, Int64)
    @test Dict{String,Int}.parameters == Core.svec(String, Int64)

    # --- Result identity / type stays Core.SimpleVector (svec) ----------------
    @test typeof(Vector{Int}.parameters) === Core.SimpleVector
    @test typeof(Val{5}.parameters) === Core.SimpleVector
    @test isa(Array{Int,3}.parameters, Core.SimpleVector)
    @test isa(Val{:foo}.parameters, Core.SimpleVector)

    # --- Splat of a value-parameter-bearing svec ------------------------------
    pair(a, b) = (a, b)
    @test pair(Vector{Int}.parameters...) === (Int64, 1)
    @test pair(Array{Int,3}.parameters...) === (Int64, 3)

    # --- getfield form matches dot form ---------------------------------------
    @test getfield(Vector{Int}, :parameters) == Core.svec(Int64, 1)
    @test getfield(Matrix{Float64}, :parameters) == Core.svec(Float64, 2)

    # --- Display form: svec(...) with value parameters ------------------------
    @test string(Vector{Int}.parameters) == "svec(Int64, 1)"
    @test string(Array{Int,3}.parameters) == "svec(Int64, 3)"
    @test string(Val{:foo}.parameters) == "svec(:foo)"
    @test string(Val{true}.parameters) == "svec(true)"

    # --- Dynamic field-access path (t typed as Any) yields value params too ---
    getparams(t) = t.parameters
    @test getparams(Vector{Int}) == Core.svec(Int64, 1)
    @test typeof(getparams(Vector{Int})) === Core.SimpleVector
end

true
