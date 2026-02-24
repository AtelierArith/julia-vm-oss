# Test varargs macro support
# Macros with rest... parameters should collect remaining arguments

# Basic varargs macro - returns the varargs tuple
macro return_rest(x, rest...)
    quote
        $rest
    end
end

# Test with 2 varargs - verify using element access
result1 = @return_rest(100, 200, 300)
@assert length(result1) == 2 "Should have 2 varargs"
@assert result1[1] == 200 "First vararg should be 200"
@assert result1[2] == 300 "Second vararg should be 300"

# Test with 3 varargs
result2 = @return_rest(1, 2, 3, 4)
@assert length(result2) == 3 "Should have 3 varargs"
@assert result2[1] == 2 "First vararg"
@assert result2[2] == 3 "Second vararg"
@assert result2[3] == 4 "Third vararg"

# Varargs macro with 2 fixed args
macro get_rest(x, y, rest...)
    quote
        $rest
    end
end

result3 = @get_rest(1, 2, 3, 4)
@assert length(result3) == 2 "Should have 2 varargs after 2 fixed"
@assert result3[1] == 3 "First vararg"
@assert result3[2] == 4 "Second vararg"

# Multiple elements in varargs
result4 = @get_rest(10, 20, 30, 40, 50)
@assert length(result4) == 3 "Should have 3 varargs"
@assert result4[1] == 30 "First"
@assert result4[2] == 40 "Second"
@assert result4[3] == 50 "Third"

# Single vararg element
result5 = @return_rest(1, 2)
@assert length(result5) == 1 "Single vararg"
@assert result5[1] == 2 "Value should be 2"

# Varargs with 5 extra args
result6 = @return_rest(0, 1, 2, 3, 4, 5)
@assert length(result6) == 5 "Varargs should have correct length"

# Access varargs elements
result7 = @return_rest(0, 10, 20, 30)
@assert result7[1] == 10 "First vararg element"
@assert result7[2] == 20 "Second vararg element"
@assert result7[3] == 30 "Third vararg element"

# Return a numeric result for test fixture compatibility
42.0
