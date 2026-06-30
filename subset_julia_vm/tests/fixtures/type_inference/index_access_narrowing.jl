# Issue #3521: Index access narrowing — arr[i] (with constant i) should be
# inferred from the refined type after `isa(arr[i], T)` or `arr[i] !== nothing`.
function f(xs::Vector{Int64})
    if xs[1] isa Int64
        return xs[1] + 1
    end
    return 0
end

@assert f([41]) == 42

# Tuple-style destructured narrowing on first/last
function g(t::Tuple{Int64, Int64})
    if t[1] isa Int64
        return t[1] + t[2]
    end
    return 0
end

@assert g((10, 20)) == 30

true
