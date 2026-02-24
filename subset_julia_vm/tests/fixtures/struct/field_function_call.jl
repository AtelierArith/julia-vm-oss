# Test calling a function stored in a struct field
# This tests the g.f(x) pattern where f is a Function field

# Define a simple function to store
add_one(x) = x + 1

# Struct with a Function field
struct Container
    f::Function
end

# Basic case: call function stored in field
c = Container(add_one)
result1 = c.f(5)
result2 = c.f(10)

# Verify results
result1 == 6 && result2 == 11
