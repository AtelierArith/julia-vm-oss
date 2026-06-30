ex = :(return 42)

ex isa Expr &&
    ex.head === :return &&
    length(ex.args) == 1 &&
    ex.args[1] == 42
