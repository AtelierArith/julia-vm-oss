struct BatchIntegrand8407{Y, X, F}
    f!::F
    y::Vector{Y}
    x::X
end

BatchIntegrand8407(f!, y::Vector{Y}) where {Y} =
    BatchIntegrand8407{Y, Nothing, typeof(f!)}(f!, y, nothing)

quad8407(x, ys...; maxevals=0) = 10
quad8407(x::BatchIntegrand8407{Y, Nothing}, a::T, b::T, rest::T...; kws...) where {Y, T} = 20

struct ReturnSegbuf8407{S}
    segbuf::S
end

quad_forward8407(f, segs...; kws...) =
    quad_forward8407(f, promote(segs...)...; kws...)
quad_forward8407(f, segs::T...; segbuf=nothing, kws...) where {T} = 30
quad_segbuf8407(args...; segbuf=nothing, kws...) =
    quad_forward8407(args...; segbuf=ReturnSegbuf8407(segbuf), kws...)

function runtime_kwargs_vararg_partial_param_contract_8407()
    f!(y, x) = y
    bi = Any[BatchIntegrand8407(f!, Float64[])][1]
    ok_isa = bi isa BatchIntegrand8407{Float64, Nothing}
    ok_dispatch = quad8407(bi, 0.0, 1.0, maxevals=1) == 20
    ok_kw_forward = quad_segbuf8407(identity, 0.0, 1.0) == 30
    ok_isa && ok_dispatch && ok_kw_forward
end

runtime_kwargs_vararg_partial_param_contract_8407()
