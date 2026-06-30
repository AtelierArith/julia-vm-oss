rmlines(x) = x
prewalk(f, x) = f(x)
striplines(ex) = prewalk(rmlines, ex)

macro q(ex)
    esc(Expr(:quote, striplines(ex)))
end

(@q 1) == 1
