using Test

function ts_array_4291(xs::Vector{Int64})
    map(x -> x + 1, xs)
end

ts_tuple_4291(t::Tuple{Int64,Float64}) = map(x -> x + 1, t)
ts_generator_4291(xs::Vector{Int64}) = collect(x + 1 for x in xs)

function ts_closure_generator_4291(a::Int64)
    f(x) = x + a
    collect(f(x) for x in 1:3)
end

# Issue #9990: these overloads are both visible before the caller below, so
# source-world runtime dispatch must not widen the caller's inferred return type.
ts_dispatch_4291(x::Integer) = 1
ts_dispatch_4291(x::Number) = 1.0
ts_dispatch_caller_4291(x::Int64) = ts_dispatch_4291(x)

struct TSBoxInner4291
    x::Int64
    TSBoxInner4291(x) = new(x + 1)
end

make_inner_4291() = TSBoxInner4291(40)
field_inner_4291() = make_inner_4291().x

@testset "type-stability report parity source cases" begin
    @test Base.infer_return_type(ts_array_4291, Tuple{Vector{Int64}}) == Vector{Int64}
    @test Base.infer_return_type(ts_tuple_4291, Tuple{Tuple{Int64,Float64}}) == Tuple{Int64,Float64}
    @test Base.infer_return_type(ts_generator_4291, Tuple{Vector{Int64}}) == Vector{Int64}
    @test Base.infer_return_type(ts_closure_generator_4291, Tuple{Int64}) == Vector{Int64}
    @test Base.infer_return_type(ts_dispatch_caller_4291, Tuple{Int64}) === Int64
    @test ts_array_4291([1, 2]) == [2, 3]
    @test ts_tuple_4291((1, 2.0)) == (2, 3.0)
    @test ts_generator_4291([1, 2]) == [2, 3]
    @test ts_closure_generator_4291(10) == [11, 12, 13]
    @test ts_dispatch_caller_4291(1) == 1
    @test field_inner_4291() == 41
end

true
