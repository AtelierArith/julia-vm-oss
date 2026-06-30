using Test

function cfg_return_letblock_5602(flag::Bool, x::Int64)
    if flag
        return let y = x + 1
            y
        end
    else
        return let y = x + 2
            y
        end
    end
end

@test Base.infer_return_type(
    cfg_return_letblock_5602,
    Tuple{Bool,Int64},
) === Int64

true
