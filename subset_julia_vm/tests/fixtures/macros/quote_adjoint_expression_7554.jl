ex = :(x')

ex isa Expr && ex.head === Symbol("'") && ex.args == [:x]
