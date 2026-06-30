module MacroLocalVisibility7525
export getvalue

macro m(ex)
    esc(ex)
end

value = 0
@m value = 123

getvalue() = value
end

using .MacroLocalVisibility7525
println(getvalue())

true
