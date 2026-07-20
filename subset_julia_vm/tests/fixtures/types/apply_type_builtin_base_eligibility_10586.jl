# Core.apply_type / literal type-application eligibility for builtin bases.
#
# Issue #10587: `Core.apply_type` requires a UnionAll base. A bare
# non-parametric concrete/abstract type (Int64, Real, a non-parametric struct)
# must raise TypeError, not silently build a nonsense `Int64{Float64}`.
#
# Issue #10586: an under-applied builtin parametric family (`Array{Float64}`,
# `Dict{String}`) is a trailing UnionAll (`Array{Float64, N} where N`), so it is
# `isa UnionAll`, has `typeof == UnionAll`, and can take its remaining parameter
# (`Array{Float64}{2} === Matrix{Float64}`). Construction from that form still
# works because the `::Type{Array{T}}` dispatch binds `T` through the
# trailing-UnionAll form.

using Test

struct NonParamBase10587 end
abstract type NonParamAbstract10587 end

@testset "Core.apply_type bare non-parametric base raises TypeError (Issue #10587)" begin
    # Bare concrete builtin.
    @test_throws TypeError Core.apply_type(Int64, Float64)
    @test_throws TypeError Core.apply_type(Bool, Int64)
    @test_throws TypeError Core.apply_type(String, Int64)
    # Bare abstract builtin (no parametric schema).
    @test_throws TypeError Core.apply_type(Real, Int64)
    @test_throws TypeError Core.apply_type(Number, Int64)
    # Non-parametric user struct / abstract type.
    @test_throws TypeError Core.apply_type(NonParamBase10587, Int64)
    @test_throws TypeError Core.apply_type(NonParamAbstract10587, Int64)

    err = try
        Core.apply_type(Int64, Float64)
        nothing
    catch e
        e
    end
    @test typeof(err) == TypeError

    # A bare non-parametric base is rejected even when it reaches apply_type only
    # at runtime through a variable.
    b = Int64
    @test_throws TypeError Core.apply_type(b, Float64)

    # #10422 is preserved: a fully-applied concrete parametric base still errors.
    @test_throws TypeError Core.apply_type(Vector{Int64}, Float64)
    @test_throws TypeError Core.apply_type(Matrix{Float64}, 2)
end

@testset "under-applied builtin family is a trailing UnionAll (Issue #10586)" begin
    A = Array{Float64}
    @test A isa UnionAll
    @test typeof(A) == UnionAll
    @test A === Core.apply_type(Array, Float64)
    @test Core.apply_type(Array, Float64) isa UnionAll

    # Applying the remaining parameter yields the concrete DataType.
    @test Array{Float64}{2} === Matrix{Float64}
    @test Array{Float64}{1} === Vector{Float64}
    @test Core.apply_type(Array{Float64}, 2) === Matrix{Float64}
    @test Core.apply_type(A, 2) === Matrix{Float64}

    # Same for a two-parameter family with one bound prefix parameter.
    D = Dict{String}
    @test D isa UnionAll
    @test typeof(D) == UnionAll
    @test D === Core.apply_type(Dict, String)
    @test Dict{String}{Int64} === Dict{String,Int64}
    @test Core.apply_type(Dict{String}, Int64) === Dict{String,Int64}

    # A fully-applied family is a concrete DataType, not a UnionAll.
    @test !(Vector{Float64} isa UnionAll)
    @test !(Array{Float64,2} isa UnionAll)
end

# The under-applied form must remain constructible: the `::Type{Array{T}}`
# constructor dispatch binds `T` from the trailing UnionAll.
@testset "construction from under-applied builtin family still works (Issue #10586)" begin
    @test Array{Float64}(undef, 3) isa Vector{Float64}
    @test Array{Int64}(undef, 2, 2) isa Matrix{Int64}
    @test Vector{Float64}(undef, 3) isa Vector{Float64}
    @test typeof(zeros(Float64, 3)) === Vector{Float64}
    @test typeof(zeros(Int64, 2, 2)) === Matrix{Int64}
    @test eltype(Array{Float64}) === Float64

    # A method that binds the element type through `::Type{Array{T}}`.
    elem(::Type{Array{T}}) where {T} = T
    @test elem(Array{Float64}) === Float64
    @test elem(Array{Int64}) === Int64
end

# The variable form `A{2}` resolves through the runtime UnionAll value inside a
# function scope (its top-level type-alias interaction is tracked separately).
function apply_in_scope_10586()
    A = Array{Float64}
    return A{2}
end

@testset "variable-form application in function scope (Issue #10586)" begin
    @test apply_in_scope_10586() === Matrix{Float64}
end

true
