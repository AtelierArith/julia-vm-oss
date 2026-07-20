using Test

macro m9471(x)
    :($x + 1)
end

expanded = @macroexpand1 @m9471(2)

@test expanded isa Expr
@test expanded.head == :call
@test expanded.args[2] == 2
@test expanded.args[3] == 1
@test eval(expanded) == 3

true
