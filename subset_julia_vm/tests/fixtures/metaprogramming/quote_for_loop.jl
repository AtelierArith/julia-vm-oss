# Test quote expansion for :for loops

# Simple macro that generates a for loop with inline body
macro sum_n(n)
    quote
        local _sum = 0
        for _i in 1:$(esc(n))
            _sum = _sum + _i
        end
        _sum
    end
end

# Test the macro
result = @sum_n 5
println("result of @sum_n 5: ", result)
# Expected: 15 (1+2+3+4+5)

# Verify result
result == 15
