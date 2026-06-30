function generated_assignments(bs)
    quote
        $((:($b = 1) for b in bs)...)
    end
end

ex = generated_assignments([:x])
ex isa Expr &&
    ex.head === :block &&
    length(ex.args) == 2 &&
    ex.args[2] == :(x = 1)
