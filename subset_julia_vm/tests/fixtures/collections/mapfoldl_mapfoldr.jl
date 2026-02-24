# Test mapfoldl() and mapfoldr() functions
# mapfoldl(f, op, itr) - apply f to each element, then left-fold with op
# mapfoldr(f, op, itr) - apply f to each element, then right-fold with op
# Note: Tests with init argument are not included since Julia uses keyword args
#       while SubsetJuliaVM uses positional args (API difference)

result = 0.0

# Define wrapper functions
square(x) = x * x
identity_fn(x) = x
negate(x) = -x
add(x, y) = x + y
sub(x, y) = x - y
mul(x, y) = x * y

arr = [1.0, 2.0, 3.0]

# Test 1: mapfoldl with square and add (should be same as mapreduce)
# mapfoldl(square, add, [1, 2, 3]) = 1^2 + 2^2 + 3^2 = 14
mfl_result = mapfoldl(square, add, arr)
if mfl_result == 14.0
    result = result + 1.0
end

# Test 2: mapfoldl with square and subtraction
# mapfoldl(square, sub, [1, 2, 3]) = ((1^2 - 2^2) - 3^2) = ((1 - 4) - 9) = -12
mfl_sub = mapfoldl(square, sub, arr)
if mfl_sub == -12.0
    result = result + 1.0
end

# Test 3: mapfoldr with square and subtraction
# mapfoldr(square, sub, [1, 2, 3]) = (1^2 - (2^2 - 3^2)) = (1 - (4 - 9)) = (1 - (-5)) = 6
mfr_sub = mapfoldr(square, sub, arr)
if mfr_sub == 6.0
    result = result + 1.0
end

# Test 4: mapfoldr with identity and subtraction
# mapfoldr(identity_fn, sub, [1, 2, 3]) = (1 - (2 - 3)) = (1 - (-1)) = 2
mfr_id = mapfoldr(identity_fn, sub, arr)
if mfr_id == 2.0
    result = result + 1.0
end

# Test 5: mapfoldl with single element
arr_single = [5.0]
mfl_single = mapfoldl(square, add, arr_single)
if mfl_single == 25.0
    result = result + 1.0
end

# Test 6: mapfoldr with single element
mfr_single = mapfoldr(square, add, arr_single)
if mfr_single == 25.0
    result = result + 1.0
end

result
