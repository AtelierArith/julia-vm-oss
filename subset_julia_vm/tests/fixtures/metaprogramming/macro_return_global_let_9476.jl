using Test

macro loopit9476(forloop)
    newbody = Expr(:block, forloop.args[2], Expr(:global, :_c9476), :(_c9476 += 1))
    newloop = Expr(forloop.head, forloop.args[1], newbody)
    esc(quote
        _c9476 = 1
        $newloop
        _c9476
    end)
end

macro mklet9476()
    body = quote
        x9476 = 1
        x9476 += 3
        x9476
    end
    Expr(:let, Expr(:block), body)
end

_c9476 = 0
local_seen = @loopit9476 for i in 1:3
    nothing
end

@test local_seen == 4
@test _c9476 == 4
let_seen = @mklet9476
@test let_seen == 4

true
