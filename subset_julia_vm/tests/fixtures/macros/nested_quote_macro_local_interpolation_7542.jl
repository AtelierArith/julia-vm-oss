macro q(ex)
    esc(Expr(:quote, ex))
end

function local_quote_value()
    y = :foo
    return @q $y
end

local_quote_value() === :foo
