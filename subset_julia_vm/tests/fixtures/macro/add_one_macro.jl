# Test macro with binary operation in body
# The macro body is: x + 1
# When called with @addone 41, x is substituted with 41, giving 41 + 1 = 42

macro addone(x)
    x + 1
end

result = @addone 41
Float64(result)
