# Test @assert macro with message argument and various comparison operators
# This tests the fix for issue #307 where @assert ex1 === ex2 "msg" would fail
# when there's a space between the condition and the message.

# Test 1: Basic literals with message
@assert 1 == 1 "one equals one"
@assert 1 === 1 "one is one"

# Test 2: Variables with comparison operators and message
a = 1
b = 1
@assert a == b "a equals b"
@assert a === b "a is b"

# Test 3: Same variable with === and message (the original bug case from issue #307)
# Note: Using the same variable for === test because in Julia, two separately
# created Expr objects are == but not === (different memory locations)
ex1 = :(1 + 2)
ex2 = ex1  # Same object, so ex1 === ex2 is true
@assert ex1 === ex2 "same expression variable should be identical"

# Test 4: Multiple operators with message
@assert 2 > 1 "2 is greater than 1"
@assert 1 < 2 "1 is less than 2"
@assert 2 >= 2 "2 is greater or equal to 2"
@assert 2 <= 2 "2 is less or equal to 2"
@assert 1 != 2 "1 is not equal to 2"

# Test 5: Boolean expressions with message
@assert true && true "true and true"
@assert true || false "true or false"

# Test 6: Function call results with message
function always_true()
    true
end
@assert always_true() "function returns true"

# Final result - all tests passed
42
