# Test empty() function - create empty collection of same type
# empty(collection) -> empty collection of same type

result = 0.0

# Test empty for array
arr = [1, 2, 3]
empty_arr = empty(arr)
if length(empty_arr) == 0
    result = result + 1.0
end

# Test empty for Dict (using Dict() constructor)
dict = Dict()
dict["a"] = 1
dict["b"] = 2
empty_dict = empty(dict)
if length(empty_dict) == 0
    result = result + 1.0
end

# Test empty for Tuple
tup = (1, 2, 3)
empty_tup = empty(tup)
if length(empty_tup) == 0
    result = result + 1.0
end

# Test empty for Set (Issue #1824)
s = Set([1, 2, 3])
empty_s = empty(s)
if length(empty_s) == 0
    result = result + 1.0
end

result
