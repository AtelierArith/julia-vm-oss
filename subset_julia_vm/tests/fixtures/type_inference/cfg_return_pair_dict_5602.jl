using Test

function cfg_return_pair_5602(flag::Bool, x::Int64, y::Float64)
    if flag
        return x => y
    else
        return (x + 1) => y
    end
end

function cfg_return_dict_5602(flag::Bool, x::Int64, y::Float64)
    if flag
        return Dict(x => y)
    else
        return Dict((x + 1) => y)
    end
end

@test Base.infer_return_type(
    cfg_return_pair_5602,
    Tuple{Bool,Int64,Float64},
) == Pair{Int64,Float64}
@test Base.infer_return_type(
    cfg_return_dict_5602,
    Tuple{Bool,Int64,Float64},
) == Dict{Int64,Float64}

true
