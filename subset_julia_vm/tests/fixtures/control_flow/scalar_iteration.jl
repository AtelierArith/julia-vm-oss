# Scalar iteration: iterating over a single number should yield the number once (Issue #4)
# In Julia, numbers are iterable: for x in 6; println(x); end prints 6

total = 0
for x in 6
    total = total + x
end

# total should be 6
total