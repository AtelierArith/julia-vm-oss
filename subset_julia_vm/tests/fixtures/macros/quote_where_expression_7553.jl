ex1 = :(x where {T})
ex2 = :(x where {T, S})
ex3 = :(f(a::T) where T)
ex4 = :(f(a::T) where {T})

ex1 isa Expr &&
    ex1.head === :where &&
    ex1.args == [:x, :T] &&
    ex2 isa Expr &&
    ex2.head === :where &&
    ex2.args == [:x, :T, :S] &&
    ex3 isa Expr &&
    ex3.head === :where &&
    ex3.args == [:(f(a::T)), :T] &&
    ex4 isa Expr &&
    ex4.head === :where &&
    ex4.args == [:(f(a::T)), :T]
