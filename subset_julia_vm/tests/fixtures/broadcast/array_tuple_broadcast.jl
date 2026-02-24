# Array-Tuple mixed broadcast: returns array
# [1,2,3] .+ (4,5,6) = [5,7,9]

# Array .+ Tuple
a = [1.0, 2.0, 3.0] .+ (4.0, 5.0, 6.0)
sum_a = a[1] + a[2] + a[3]  # 5+7+9 = 21

# Tuple .+ Array
b = (1.0, 2.0, 3.0) .+ [4.0, 5.0, 6.0]
sum_b = b[1] + b[2] + b[3]  # 5+7+9 = 21

sum_a + sum_b  # 21 + 21 = 42
