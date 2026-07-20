# Test: @something macro - return first non-nothing value
# This tests the Pure Julia implementation of @something

# Test 1: First argument is not nothing
r1 = @something(1, 2, 3) == 1

# Test 2: First argument is nothing, second is not
r2 = @something(nothing, 42) == 42

# Test 3: Multiple nothings, last non-nothing wins
r3 = @something(nothing, nothing, 99) == 99

# Test 4: All values are non-nothing, first wins
r4 = @something(10, 20, 30) == 10

# Test 5: Single argument
r5 = @something(5) == 5

# Test 6: Using variables
x = nothing
y = 100
r6 = @something(x, y) == 100

# Test 7: Nested @something
r7 = @something(nothing, @something(nothing, 7)) == 7

# Sum all results: 7 tests
Float64(r1) + Float64(r2) + Float64(r3) + Float64(r4) + Float64(r5) + Float64(r6) + Float64(r7)
