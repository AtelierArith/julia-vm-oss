# Test: Named kwargs combined with kwargs...
# Named kwargs are matched first, remaining go to kwargs...

function h(; x=0, kwargs...)
    return x + length(kwargs)
end

result1 = h(x=5)           # x=5, kwargs=()
result2 = h(x=5, a=1, b=2) # x=5, kwargs=(a=1, b=2)

result1 == 5 && result2 == 7
