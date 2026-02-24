# Test filter! function (in-place filter)
# filter!(f, arr) removes elements where f returns false

# Basic filter!
arr1 = [1.0, 2.0, 3.0, 4.0, 5.0]
filter!(x -> x > 2, arr1)
println(length(arr1))  # 3
println(arr1[1])  # 3.0
println(arr1[2])  # 4.0
println(arr1[3])  # 5.0

# Filter to keep even numbers
arr2 = [1.0, 2.0, 3.0, 4.0, 6.0, 8.0]
filter!(x -> mod(x, 2) == 0, arr2)
println(length(arr2))  # 4
println(arr2[1])  # 2.0
println(arr2[2])  # 4.0

# All elements match
arr3 = [2.0, 4.0, 6.0]
filter!(x -> x > 0, arr3)
println(length(arr3))  # 3

# No elements match
arr4 = [1.0, 2.0, 3.0]
filter!(x -> x > 10, arr4)
println(length(arr4))  # 0
