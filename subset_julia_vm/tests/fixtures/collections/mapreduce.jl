# Test mapreduce() function
# mapreduce(f, op, itr) - apply f to each element, then reduce with op
# Note: Tests with init argument are not included since Julia uses keyword args
#       while SubsetJuliaVM uses positional args (API difference)

result = 0.0

# Define wrapper functions
square(x) = x * x
identity_fn(x) = x
negate(x) = -x
add(x, y) = x + y
mul(x, y) = x * y

# Test mapreduce with squaring and summing
# mapreduce(square, add, [1, 2, 3]) = 1^2 + 2^2 + 3^2 = 1 + 4 + 9 = 14
arr = [1.0, 2.0, 3.0]
mr_result = mapreduce(square, add, arr)
if mr_result == 14.0
    result = result + 1.0
end

# Test mapreduce with identity and multiplication
# mapreduce(identity_fn, mul, [2, 3, 4]) = 2 * 3 * 4 = 24
arr2 = [2.0, 3.0, 4.0]
mr_prod = mapreduce(identity_fn, mul, arr2)
if mr_prod == 24.0
    result = result + 1.0
end

# Test mapreduce with single element array
arr_single = [5.0]
mr_single = mapreduce(square, add, arr_single)
if mr_single == 25.0
    result = result + 1.0
end

# Test mapreduce with negate and add
# mapreduce(negate, add, [1, 2, 3]) = -1 + -2 + -3 = -6
mr_negate = mapreduce(negate, add, arr)
if mr_negate == -6.0
    result = result + 1.0
end

result
