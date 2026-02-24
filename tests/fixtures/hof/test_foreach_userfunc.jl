# Test foreach with user-defined function
# Regression test for #721: foreach(show, [1,2,3]) failed with 'm' not defined
# when a user-defined function with the same name as a prelude function existed

# Define a user function with a common name that also exists in prelude
# The prelude has show(io::IO, x::T) with 2 params
# User defines show(x) with 1 param
show(x) = x * 2

# Test foreach with the user-defined function that prints
myprint(x) = println(x)
foreach(myprint, [1, 2, 3])
# Expected output: 1, 2, 3 (one per line)

# Test map with user function having same name as prelude function
arr = map(show, [1, 2, 3])
println(arr[1])  # 2
println(arr[2])  # 4
println(arr[3])  # 6

# Test that direct call still works
println(show(5))  # 10
