# Test Expr.head and Expr.args field access
# In Julia, Expr is defined in Core with head::Symbol and args::Vector{Any}

# Test expr.head returns a Symbol
expr = :(a + b)
@assert expr.head == :call

# Test expr.args returns an array
@assert length(expr.args) == 3
# Note: Comparing array elements (Any type) to Symbols needs
# dynamic dispatch improvement, tested separately

# Test block expression
block = quote
    x = 1
    y = 2
end
@assert block.head == :block

# Return success value
42.0
