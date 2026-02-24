# Test: Accessing kwargs fields
# kwargs is Base.Pairs, so we use symbol indexing (not dot notation)

function g(; kwargs...)
    return kwargs[:a] + kwargs[:b]
end

result = g(a=10, b=20)
result == 30
