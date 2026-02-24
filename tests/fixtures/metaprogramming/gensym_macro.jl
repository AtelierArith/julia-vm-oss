# Test @gensym macro - generates unique symbol names

# Basic usage: single symbol
@gensym x
println("x value: ", x)
println("x type: ", typeof(x))

# Verify the symbol has the expected prefix
x_str = string(x)
println("x as string: ", x_str)
@assert startswith(x_str, "##x#") "Expected x to start with ##x#"

# Multiple symbols
@gensym a b c
println("a: ", string(a))
println("b: ", string(b))
println("c: ", string(c))

# Verify all symbols are unique (using == false instead of !=)
@assert (a == b) == false "Symbols a and b should be different"
@assert (b == c) == false "Symbols b and c should be different"
@assert (a == c) == false "Symbols a and c should be different"

# Verify symbols have correct prefixes
@assert startswith(string(a), "##a#")
@assert startswith(string(b), "##b#")
@assert startswith(string(c), "##c#")

println("All @gensym tests passed!")
