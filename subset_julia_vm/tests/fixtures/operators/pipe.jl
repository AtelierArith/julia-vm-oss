# Test pipe operator |>
# x |> f applies function f to argument x

# Test with single pipe
check1 = (4 |> abs) == 4
check2 = (-4 |> abs) == 4

# Test with chained pipes
# 16 |> sqrt |> sqrt = sqrt(sqrt(16)) = sqrt(4) = 2
check3 = (16.0 |> sqrt |> sqrt) == 2.0

# Test with user-defined function
double(x) = x * 2
check4 = (5 |> double) == 10

# Test multiple pipes with user-defined functions
add_one(x) = x + 1
check5 = (3 |> double |> add_one) == 7  # double(3) = 6, add_one(6) = 7

# Test with arrays
check6 = ([1, 2, 3] |> length) == 3

# All checks must pass
check1 && check2 && check3 && check4 && check5 && check6
