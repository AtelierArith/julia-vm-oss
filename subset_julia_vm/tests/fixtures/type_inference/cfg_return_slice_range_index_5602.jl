using Test

function cfg_return_slice_all_5602(flag::Bool, m::Matrix{Int64})
    if flag
        return m[1, :]
    else
        return m[:, 1]
    end
end

function cfg_return_range_index_5602(flag::Bool, m::Matrix{Int64})
    if flag
        return m[1:2, 1]
    else
        return m[2:3, 1]
    end
end

@test Base.infer_return_type(cfg_return_slice_all_5602, Tuple{Bool,Matrix{Int64}}) === Vector{Int64}
@test Base.infer_return_type(cfg_return_range_index_5602, Tuple{Bool,Matrix{Int64}}) === Vector{Int64}

true
