# Test HOF return type inference
# map should preserve/infer element types correctly

result = 0.0

# map with Int64 -> Int64 function
square(x) = x * x
arr = [1, 2, 3]
mapped = map(square, arr)
if eltype(mapped) === Int64
    result += 1.0
end

# filter preserves input element type
evens = filter(x -> x % 2 == 0, [2, 4, 6, 8])
if eltype(evens) === Int64
    result += 1.0
end

# Verify map result values are correct
if sum(mapped) == 14  # 1 + 4 + 9 = 14
    result += 1.0
end

result
