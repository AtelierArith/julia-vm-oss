ex = :(a, b)

println(Base.isexpr(ex, :tuple))
println(Base.isexpr(ex, :tuple, 2))
println(!Base.isexpr(ex, :call))

true
