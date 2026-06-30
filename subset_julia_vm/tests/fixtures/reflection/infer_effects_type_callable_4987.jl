using Test

# Issue #4987: the VM-backed reflection helper rejected `DataType` callables
# (constructors such as `Int64`, `Bool`, `Float64`) with
# "Expected function, string, or symbol" before the pure-Julia effect
# classifier could run. `extract_func_name` now keys a `DataType` callable by
# its type name, mirroring `nameof`, so `methods` / `which` / `hasmethod` and
# the `infer_effects` / `infer_exception_type` surface all accept type
# callables. Expected values captured from upstream Julia 1.12.

@testset "infer_effects two-arg type callable (#4987)" begin
    # Int64 / Bool conversions from floating inputs can throw InexactError.
    @test Base.infer_exception_type(Int64, Tuple{Float64}) === InexactError
    @test Base.infer_effects(Int64, Tuple{Float64}).nothrow == false
    @test Base.infer_exception_type(Bool, Tuple{Float64}) === InexactError
    @test Base.infer_effects(Bool, Tuple{Float64}).nothrow == false

    # Float64 from an integer input is total and cannot throw.
    @test Base.infer_effects(Float64, Tuple{Int64}).nothrow == true
    @test Base.infer_exception_type(Float64, Tuple{Int64}) === Union{}
end

@testset "reflection helpers accept type callables (#4987)" begin
    # Previously these raised "Type error: Expected function, string, or
    # symbol"; they must now resolve without throwing a TypeError.
    @test methods(Int64) isa AbstractVector
    @test methods(Int64, Tuple{Float64}) isa AbstractVector
    @test methods(Float64) isa AbstractVector

    # Single-argument infer_effects / infer_exception_type on a type callable
    # must reach the classifier instead of erroring.
    @test Base.infer_effects(Int64).nothrow isa Bool
    @test Base.infer_exception_type(Int64) isa Type
    @test Base.infer_effects(Float64).nothrow isa Bool
    @test Base.infer_exception_type(Float64) isa Type
end

true
