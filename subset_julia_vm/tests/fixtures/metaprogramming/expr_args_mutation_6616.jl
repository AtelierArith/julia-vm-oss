# Expr.args is the mutable args vector owned by the Expr, not a detached copy.

ex = Expr(:block)
args = ex.args

push!(args, :(x = 1))
len_after_alias_push = length(ex.args)

push!(ex.args, :(y = 2))
len_after_field_push = length(args)

len_after_alias_push == 1 && len_after_field_push == 2 && length(ex.args) == 2
