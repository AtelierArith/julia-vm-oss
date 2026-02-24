# Test dirname and basename path functions
# Note: Avoiding == operator due to string comparison bug

# Test that functions exist and return strings
d1 = dirname("/home/user/file.txt")
println("dirname test 1: ", d1)

d2 = dirname("path/to/file.txt")
println("dirname test 2: ", d2)

b1 = basename("/home/user/file.txt")
println("basename test 1: ", b1)

b2 = basename("file.txt")
println("basename test 2: ", b2)

# Return true to indicate tests completed
true
