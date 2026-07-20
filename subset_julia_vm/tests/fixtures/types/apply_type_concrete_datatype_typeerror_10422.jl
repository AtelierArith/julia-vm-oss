# Core.apply_type on a concrete/non-UnionAll DataType raises TypeError
# (upstream jl_apply_type requires a UnionAll base) instead of silently
# rebuilding a type from the bare base name. Issues #10422 and #10587.

using Test

struct PairApply10422{A,B}
    a::A
    b::B
end

struct PlainApply10587
    x::Int64
end

make_vec_base_10422(flag) = flag ? Vector{Int64} : Vector{Float32}
make_plain_base_10587(flag) = flag ? Int64 : PlainApply10587

@testset "Core.apply_type concrete DataType base raises TypeError (Issues #10422/#10587)" begin
    # MWE: literal concrete parametric instantiation as the base.
    err = try
        Core.apply_type(Vector{Int64}, Float64)
        nothing
    catch e
        e
    end
    @test typeof(err) == TypeError

    @test_throws TypeError Core.apply_type(Vector{Int64}, Float64)
    @test_throws TypeError Core.apply_type(Dict{String,Int64}, Float64)
    @test_throws TypeError Core.apply_type(Complex{Float64}, Float64)
    @test_throws TypeError Core.apply_type(Tuple{Int64}, Float64)
    @test_throws TypeError Core.apply_type(PairApply10422{Int64,Float64}, String)
    @test_throws TypeError Core.apply_type(Int64, Float64)
    @test_throws TypeError Core.apply_type(Real, Int64)
    @test_throws TypeError Core.apply_type(PlainApply10587, Float64)

    # Concrete base traced through a local variable (the compiler used to
    # constant-fold this onto the same name-stripping static path).
    w = Vector{Int64}
    @test_throws TypeError Core.apply_type(w, Float64)

    # Concrete base only known at runtime.
    b = make_vec_base_10422(true)
    @test_throws TypeError Core.apply_type(b, Float64)
    c = make_plain_base_10587(true)
    @test_throws TypeError Core.apply_type(c, Float64)

    # Positive control: a UnionAll base still applies.
    @test Core.apply_type(Vector, Float64) === Vector{Float64}
    @test Core.apply_type(PairApply10422, Int64, Float64) ===
          PairApply10422{Int64,Float64}

    # Partial application still appends parameters instead of erroring or
    # replacing the already-bound prefix (Issue #10192).
    partial = Core.apply_type(PairApply10422, Int64)
    @test typeof(partial) == UnionAll
    @test Core.apply_type(partial, Float64) === PairApply10422{Int64,Float64}
    @test Core.apply_type(PairApply10422{Int64}, Float64) ===
          PairApply10422{Int64,Float64}
end

true
