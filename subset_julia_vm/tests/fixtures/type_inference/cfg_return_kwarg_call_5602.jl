using Test

kwret5602(; value=1.5) = value

function cfg_return_kwarg_call_5602(flag::Bool, x::Int64, y::Float64)
    if flag
        return kwret5602(value=x)
    else
        return kwret5602(value=y)
    end
end

@test Base.infer_return_type(
    cfg_return_kwarg_call_5602,
    Tuple{Bool,Int64,Float64},
) == Union{Float64,Int64}

true
