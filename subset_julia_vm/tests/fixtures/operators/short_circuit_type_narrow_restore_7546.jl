isexpr(x::Expr, ts...) = x.head in ts
isexpr(x, ts...) = false

function normalise(ex)
    isexpr(ex, :inert) && (ex = Expr(:quote, ex.args[1]))
    isexpr(ex, :kw) && (ex = Expr(:(=), ex.args...))
    return ex
end

normalise(:x) === :x
