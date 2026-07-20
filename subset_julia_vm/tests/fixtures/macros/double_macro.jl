# Test macro that doubles its argument
# Uses the x parameter in an expression

macro double(x)
    quote
        2 * $x
    end
end

result = @double 21
Float64(result)
