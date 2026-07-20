# Test multi-statement quote blocks in macros
# This verifies that macros can have quote blocks with multiple statements

macro mytest(expr)
    quote
        x = 1
        y = 2
        x + y + $expr
    end
end

# Call the multi-statement macro
result = @mytest 10

# Expected result: 1 + 2 + 10 = 13
Float64(result)
