using Test

import Base: ==

struct S
    data::Vector{UInt8}
    S(s) = new(codeunits(s))
end

==(s1::S, s2::S) = s1.data == s2.data

macro macrotools_issue_7643()
    n0 = gensym()
    n1 = gensym()
    esc(Expr(:block,
        Expr(:(=), n0, :(Dict(:a => S("foo")))),
        Expr(:(=), :s, Expr(:block,
            Expr(:(=), n1, :(getindex($n0, :a))),
            Expr(:(=), :data, :(getfield($n1, :data))),
            n1)),
        n0))
end

d = @macrotools_issue_7643

@test s == S("foo")
@test ==(s, S("foo"))
@test data === s.data
@test d[:a] == S("foo")

true
