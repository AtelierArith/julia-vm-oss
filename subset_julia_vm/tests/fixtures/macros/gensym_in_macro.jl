# Test gensym usage in macros for variable hygiene
# This tests that gensym-generated symbols work as variable names

macro swap_add(a, b)
    tmp = gensym("tmp")
    esc(quote
        $tmp = $a
        $a = $b
        $b = $tmp
        $a + $b  # Return sum after swap
    end)
end

x = 10
y = 3
result = @swap_add x y
# After swap: x = 3, y = 10
# Result should be 3 + 10 = 13

Float64(result)
