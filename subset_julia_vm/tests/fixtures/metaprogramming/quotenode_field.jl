# Test QuoteNode field access
# QuoteNode has a single field 'value' that contains the quoted value

# Create a QuoteNode using the constructor
qn = QuoteNode(:x)

# Test that QuoteNode wraps the value correctly
# getfield(qn, 1) should return the wrapped Symbol
wrapped = getfield(qn, 1)

# Check if wrapped value is the Symbol :x
result = wrapped == :x

Float64(result)
