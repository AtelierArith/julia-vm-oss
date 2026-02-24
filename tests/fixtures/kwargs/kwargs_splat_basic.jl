# Test: Basic kwargs... functionality
# kwargs... collects all remaining keyword arguments as Base.Pairs

function f(; kwargs...)
    return length(kwargs)
end

result = f(a=1, b=2, c=3)
result == 3
