# Test: typed function
# Expected: 15

function add(x::Int64, y::Int64)::Int64
    x + y
end

add(10, 5)
