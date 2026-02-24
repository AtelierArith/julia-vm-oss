# Test string(expr) - Julia code format stringification of Expr
# Julia's string() outputs content WITHOUT the :() wrapper

# Basic Symbol (no : prefix)
@assert string(:foo) == "foo"
@assert string(:bar) == "bar"
@assert string(:x) == "x"

# Basic binary operators (no :() wrapper)
@assert string(:(x + 1)) == "x + 1"
@assert string(:(a - b)) == "a - b"
@assert string(:(a * b)) == "a * b"
@assert string(:(a / b)) == "a / b"
@assert string(:(a ^ b)) == "a ^ b"

# Function calls
@assert string(:(f(a))) == "f(a)"
@assert string(:(f(a, b))) == "f(a, b)"
@assert string(:(sin(x))) == "sin(x)"

# Comparison
@assert string(:(a == b)) == "a == b"
@assert string(:(a < b)) == "a < b"
@assert string(:(a >= b)) == "a >= b"

# Tuple
@assert string(:((a, b))) == "(a, b)"

println("All string(expr) tests passed!")
