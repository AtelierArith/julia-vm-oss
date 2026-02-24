# Test: Positional args with kwargs...
# Combines positional arguments with keyword argument splatting

function p(x, y; kwargs...)
    return x + y + length(kwargs)
end

result = p(1, 2, a=10, b=20, c=30)
result == 6  # 1 + 2 + 3 (3 kwargs)
