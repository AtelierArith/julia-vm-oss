using Test

splatcallee5602(x::Int64, y::Float64) = x

function cfg_return_positional_splat_call_5602(flag::Bool, x::Int64, y::Float64)
    args = (x, y)
    if flag
        return splatcallee5602(args...)
    else
        return splatcallee5602(x, y)
    end
end

@test Base.infer_return_type(
    cfg_return_positional_splat_call_5602,
    Tuple{Bool,Int64,Float64},
) === Int64

true
