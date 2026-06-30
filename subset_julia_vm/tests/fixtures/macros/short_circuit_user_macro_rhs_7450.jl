macro rhs_true()
    true
end

function f(x)
    x || @rhs_true()
end

f(false) && f(true)
