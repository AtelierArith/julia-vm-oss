# Test that modules without __init__ work normally (Issue #8994)
module M
    const VALUE = 123
    function greet()
        println("hello")
    end
end

M.greet()
println(M.VALUE)
true
