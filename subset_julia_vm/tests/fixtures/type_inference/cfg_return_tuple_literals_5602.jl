using Test

function cfg_return_tuple_literal_5602(flag::Bool, x::Int64, y::Float64)
    if flag
        return (x, y)
    else
        return (x + 1, y)
    end
end

function cfg_return_namedtuple_literal_5602(flag::Bool, x::Int64)
    if flag
        return (value=x,)
    else
        return (value=x + 1,)
    end
end

@test Base.infer_return_type(
    cfg_return_tuple_literal_5602,
    Tuple{Bool,Int64,Float64},
) === Tuple{Int64,Float64}
@test Base.infer_return_type(
    cfg_return_namedtuple_literal_5602,
    Tuple{Bool,Int64},
) == NamedTuple{(:value,),Tuple{Int64}}

true
