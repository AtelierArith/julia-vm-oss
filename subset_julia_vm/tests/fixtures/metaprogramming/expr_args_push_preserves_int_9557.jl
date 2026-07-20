e = :(f())
push!(e.args, 7)

plain_any = Any[:f]
push!(plain_any, 7)

expr_args_ok = e.args[end] == 7 && typeof(e.args[end]) == Int64
plain_any_ok = plain_any[end] == 7 && typeof(plain_any[end]) == Int64

println(e.args)
println(typeof(e.args[end]))
println(expr_args_ok && plain_any_ok)

expr_args_ok && plain_any_ok
