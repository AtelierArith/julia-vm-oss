# Test foldl() and foldr() functions
# foldl(op, itr) - left-associative fold (same as reduce)
# foldr(op, itr) - right-associative fold (reverse order)
# Note: Tests with init argument are not included since Julia uses keyword args
#       while SubsetJuliaVM uses positional args (API difference)

result = 0.0

# Define wrapper functions for operators
add(x, y) = x + y
sub(x, y) = x - y

# Test foldl - left-associative (same as reduce)
arr = [1.0, 2.0, 3.0, 4.0]
foldl_result = foldl(add, arr)
# foldl(add, [1,2,3,4]) = (((1+2)+3)+4) = 10
if foldl_result == 10.0
    result = result + 1.0
end

# Test foldr - right-associative (reverse order)
foldr_result = foldr(add, arr)
# foldr(add, [1,2,3,4]) = (1+(2+(3+4))) = 10 (same result for addition)
if foldr_result == 10.0
    result = result + 1.0
end

# Test foldr with subtraction (shows difference from foldl)
# foldl(sub, [10, 2, 3]) = ((10-2)-3) = 5
# foldr(sub, [10, 2, 3]) = (10-(2-3)) = 11
arr_sub = [10.0, 2.0, 3.0]
foldl_sub = foldl(sub, arr_sub)
foldr_sub = foldr(sub, arr_sub)
if foldl_sub == 5.0 && foldr_sub == 11.0
    result = result + 1.0
end

# Test with multiplication
mul(x, y) = x * y
arr_mul = [2.0, 3.0, 4.0]
foldl_mul = foldl(mul, arr_mul)
# foldl(mul, [2,3,4]) = ((2*3)*4) = 24
if foldl_mul == 24.0
    result = result + 1.0
end

result
