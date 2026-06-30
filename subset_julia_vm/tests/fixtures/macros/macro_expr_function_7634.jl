using Test

macro generated_function()
    esc(Expr(:function, Expr(:call, :foo7634, :x), Expr(:block, :(x + 2))))
end

@generated_function

@test foo7634(10) == 12
@test foo7634(-2) == 0

true
