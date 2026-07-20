# Test macro hygiene: variables introduced by macros don't shadow user variables
# This tests that local variables in macros are automatically gensym'd

# Test 1: Simple macro with local variable - hygiene should rename 'tmp'
# so it doesn't conflict with any user variable named 'tmp'
macro addtmp(x)
    quote
        local tmp = 42
        $x + tmp  # tmp should be renamed via hygiene
    end
end

# User's variable 'tmp' should not be affected
tmp = 1000
result = @addtmp 8  # Should be 8 + 42 = 50, not 8 + 1000

Float64(result)
