using Test

function cfg_return_array_literal_5602(flag::Bool, x::Int64)
    if flag
        return [x]
    else
        return [x + 1]
    end
end

@test Base.infer_return_type(cfg_return_array_literal_5602, Tuple{Bool,Int64}) === Vector{Int64}

true
