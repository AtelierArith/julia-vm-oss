# Issue #7350 (A1): an `if`/ternary expression produced by a macro and used in
# expression (argument) position must yield the value of the taken branch,
# not silently evaluate to `nothing`.
f(x) = x

macro pick(c)
    return :(f($c ? :slider : :dropdown))
end

macro pick_block()
    return quote
        if true
            :slider
        else
            :dropdown
        end
    end
end

@pick(true) === :slider &&
    @pick(false) === :dropdown &&
    @pick_block() === :slider
