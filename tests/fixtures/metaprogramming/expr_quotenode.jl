# Test Expr constructor and QuoteNode

# =============================================================================
# Expr constructor tests
# =============================================================================

# Basic Expr with :call head
e1 = Expr(:call, :+, 1, 2)
println("e1: ", e1)
@assert e1.head == :call "Expected head to be :call"
@assert length(e1.args) == 3 "Expected 3 args"

# Expr with :block head
e2 = Expr(:block, 1, 2, 3)
println("e2: ", e2)
@assert e2.head == :block "Expected head to be :block"
@assert length(e2.args) == 3 "Expected 3 args"

# Expr with nested Exprs
inner = Expr(:call, :*, :x, 2)
outer = Expr(:call, :+, inner, 1)
println("outer: ", outer)
@assert outer.head == :call
@assert outer.args[1] == :+

# Expr with symbols
e3 = Expr(:if, :cond, :then_branch)
println("e3: ", e3)
@assert e3.head == :if
@assert e3.args[1] == :cond

# =============================================================================
# QuoteNode tests
# =============================================================================

# QuoteNode with symbol
q1 = QuoteNode(:x)
println("q1: ", q1)
println("typeof(q1): ", typeof(q1))

# QuoteNode with integer
q2 = QuoteNode(42)
println("q2: ", q2)
println("typeof(q2): ", typeof(q2))

# QuoteNode with string
q3 = QuoteNode("hello")
println("q3: ", q3)

# QuoteNode with Expr
q4 = QuoteNode(Expr(:call, :f, 1))
println("q4: ", q4)

# =============================================================================
# QuoteNode in eval - should prevent evaluation
# =============================================================================
# When eval encounters a QuoteNode, it should unwrap and return the value as-is
q_sym = QuoteNode(:x)
result = eval(q_sym)
println("eval(QuoteNode(:x)): ", result)
@assert result == :x "QuoteNode should unwrap to its value"

println("All Expr and QuoteNode tests passed!")
