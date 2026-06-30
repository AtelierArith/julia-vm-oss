macro ternary_true()
    true
end

(true ? @ternary_true() : false) && !(false ? @ternary_true() : false)
