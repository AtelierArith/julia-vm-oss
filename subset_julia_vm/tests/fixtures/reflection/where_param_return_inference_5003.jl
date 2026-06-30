using Test

# Issue #5003: `where`/value-parametrized methods were never registered as
# specializable functions, so reflection-time return-type inference fell back
# to `Any` even for the simplest identity method `id(x::T) where T = x`.
# Registering them (and treating TypeVar-annotated params as open at the
# reflection gate) lets inference re-run the body with the concrete argument
# type substituted for the type variable.

where_param_id_5003(x::T) where {T} = x
where_param_vec_first_5003(xs::Vector{T}) where {T} = xs[1]
where_param_tuple_first_5003(t::Tuple{T}) where {T} = t[1]
where_param_two_5003(x::T, y::S) where {T,S} = x

@testset "Issue #5003 where-parametrized method return inference" begin
    # Identity over a bare type variable resolves to the concrete argument type.
    @test Base.infer_return_type(where_param_id_5003, Tuple{Int64}) === Int64
    @test Base.infer_return_type(where_param_id_5003, Tuple{Float64}) === Float64
    @test Base.infer_return_type(where_param_id_5003, Tuple{String}) === String
    @test Base.return_types(where_param_id_5003, Tuple{Int64})[1] === Int64

    # TypeVar nested inside a parametric container is recovered from the element.
    @test Base.infer_return_type(where_param_vec_first_5003, Tuple{Vector{Int64}}) === Int64
    @test Base.infer_return_type(where_param_vec_first_5003, Tuple{Vector{Float64}}) === Float64

    # TypeVar nested inside a tuple parameter.
    @test Base.infer_return_type(where_param_tuple_first_5003, Tuple{Tuple{Int64}}) === Int64

    # Multiple type variables: return type follows the first parameter.
    @test Base.infer_return_type(where_param_two_5003, Tuple{Int64,String}) === Int64
    @test Base.infer_return_type(where_param_two_5003, Tuple{Float64,Int64}) === Float64
end

# Epic #5003 acceptance: the value-parameter axis. A method that returns a
# bare `where`-bound *value* parameter must have reflection inference resolve
# the value's carrier type from the concrete `Val{...}` signature. These are
# the carriers that round-trip unambiguously through the static type today.
where_param_val_5003(::Val{N}) where {N} = N

@testset "Issue #5003 value-parameter carrier return inference" begin
    @test Base.infer_return_type(where_param_val_5003, Tuple{Val{3}}) === Int64
    @test Base.infer_return_type(where_param_val_5003, Tuple{Val{1.5}}) === Float64
    @test Base.infer_return_type(where_param_val_5003, Tuple{Val{:sym}}) === Symbol
    @test Base.infer_return_type(where_param_val_5003, Tuple{Val{true}}) === Bool

    # Boundary of the delivered subsystem: a *typed-integer* value-parameter
    # carrier (`Val{UInt8(1)}` -> `UInt8`, `Val{Int32(2)}` -> `Int32`) is NOT
    # asserted here. sjulia currently drops the carrier tag when forming the
    # static type (it renders `Val{1}`, inferring `Int64`), a separate
    # type-representation gap tracked under #5616. It is intentionally absent
    # because sjulia and upstream Julia disagree, so it cannot live in a
    # both-engines parity fixture until #5616 lands.
end

true
