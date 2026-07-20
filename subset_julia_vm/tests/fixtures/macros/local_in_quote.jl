# Test local keyword in macro quote blocks
# This is needed for @time macro implementation

macro mysum(expr)
    quote
        local t = 10
        local result = $expr
        t + result
    end
end

# Test: local t = 10, result = 5, return t + result = 15
value = @mysum 5

Float64(value)
