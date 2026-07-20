ex = :([a, b] = Dict(:a => 1, :b => 2))
lhs = ex.args[1]

isa(lhs, Expr) &&
    lhs.head == :vect &&
    length(lhs.args) == 2 &&
    lhs.args[1] == :a &&
    lhs.args[2] == :b
