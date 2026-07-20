using Test

function macro_context_capture_11030(n)
    @assert true
    f = x -> x + n
    f(10)
end

function where_context_capture_11030(n::T) where {T}
    f = x -> x + n
    f(10)
end

function value_param_context_capture_11030(Vector::Type, n)
    Vector{Int64}
    f = x -> x + n
    f(10)
end

function returned_context_capture_11030(n)
    @assert true
    x -> x + n
end

function pipe_context_capture_11030(n)
    @assert true
    4 |> (x -> x + n)
end

function chained_pipe_context_capture_11030(n)
    @assert true
    4 |> (x -> x + n) |> (x -> x * n)
end

function pipe_rhs_begin_scope_11030()
    @assert true
    y = 0
    result = 1 |> begin
        y = 2
        identity
    end
    (result, y)
end

@testset "assigned arrow values retain captures with LambdaContext (Issue #11030)" begin
    @test macro_context_capture_11030(4) == 14
    @test where_context_capture_11030(4) == 14
    @test value_param_context_capture_11030(Vector, 4) == 14
    @test returned_context_capture_11030(4)(10) == 14
    @test pipe_context_capture_11030(10) == 14
    @test chained_pipe_context_capture_11030(3) == 21
    @test pipe_rhs_begin_scope_11030() == (1, 2)
end

true
