macro unary_ternary_true()
    true
end

println(!(false ? @unary_ternary_true() : false))
(!(false ? @unary_ternary_true() : false)) === true
