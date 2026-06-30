isline(x) = x isa LineNumberNode

rmlines(x) = x

function rmlines(x::Expr)
    if x.head === :macrocall && length(x.args) >= 2
        Expr(x.head, x.args[1], nothing, filter(x -> !isline(x), x.args[3:end])...)
    else
        Expr(x.head, filter(x -> !isline(x), x.args)...)
    end
end

rmlines(quote
    a = 1
end) isa Expr
