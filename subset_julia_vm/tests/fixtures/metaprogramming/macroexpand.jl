# Test @macroexpand and @macroexpand1 - returns expanded macro as Expr

# Define a macro for testing
macro double(x)
    :(2 * $x)
end

# Test @macroexpand returns an Expr
result = @macroexpand @double(5)
println("result: ", result)

# The result should be an Expr
# It should be: :(2 * 5) which is Expr(:call, :*, 2, 5)
println("result.head: ", result.head)
println("result.args[1]: ", result.args[1])
println("result.args[2]: ", result.args[2])
println("result.args[3]: ", result.args[3])

# The head should be :call
@assert result.head == :call

# The args should be [:*, 2, 5]
@assert result.args[1] == :*
@assert result.args[2] == 2
@assert result.args[3] == 5

println("@macroexpand test passed!")

# Test @macroexpand1 (single-level expansion)
result1 = @macroexpand1 @double(10)
println("result1: ", result1)

# Should have the same structure as @macroexpand for simple macros
@assert result1.head == :call
@assert result1.args[1] == :*
@assert result1.args[2] == 2
@assert result1.args[3] == 10

println("@macroexpand1 test passed!")

# Test with a macro that doesn't produce Expr
macro simple(x)
    x
end

# @macroexpand of identity macro returns the value directly
simple_result = @macroexpand @simple(42)
println("simple_result: ", simple_result)
@assert simple_result == 42

println("All @macroexpand tests passed!")

# Return true for test harness
true
