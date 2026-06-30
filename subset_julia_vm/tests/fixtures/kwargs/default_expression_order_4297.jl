function kw_default_expr_arith_4297(; x=1+2)
    return x
end

function kw_default_expr_order_4297(; a=1, b=a+1)
    return (a, b)
end

kw_default_expr_global_4297 = 40
kw_default_expr_symbol_4297 = :default

function kw_default_expr_global_ref_4297(; x=kw_default_expr_global_4297+2)
    return x
end

function kw_default_expr_global_symbol_4297(; debuginfo=kw_default_expr_symbol_4297)
    return debuginfo
end

kw_default_expr_default_num_4297() = 41
kw_default_expr_default_sym_4297() = :default
kw_default_expr_default_arg_num_4297(x) = x + 1
kw_default_expr_default_arg_sym_4297(x) = x
kw_default_expr_default_kw_num_4297(; x=41) = x + 1
kw_default_expr_default_kw_sym_4297(; debuginfo=:default) = debuginfo

function kw_default_expr_zero_arg_call_num_4297(; x=kw_default_expr_default_num_4297()+1)
    return x
end

function kw_default_expr_zero_arg_call_symbol_4297(; debuginfo=kw_default_expr_default_sym_4297())
    return debuginfo
end

function kw_default_expr_arg_call_num_4297(; x=kw_default_expr_default_arg_num_4297(41))
    return x
end

function kw_default_expr_arg_call_symbol_4297(; debuginfo=kw_default_expr_default_arg_sym_4297(:default))
    return debuginfo
end

function kw_default_expr_kw_call_num_4297(; x=kw_default_expr_default_kw_num_4297(x=41))
    return x
end

function kw_default_expr_kw_call_dep_4297(; seed=41, x=kw_default_expr_default_kw_num_4297(x=seed))
    return x
end

function kw_default_expr_kw_call_symbol_4297(; debuginfo=kw_default_expr_default_kw_sym_4297(debuginfo=:default))
    return debuginfo
end

if kw_default_expr_arith_4297() != 3
    error("non-literal arithmetic keyword default was not evaluated")
end

if kw_default_expr_arith_4297(x=9) != 9
    error("explicit keyword value did not override arithmetic default")
end

omitted = kw_default_expr_order_4297()
explicit_a = kw_default_expr_order_4297(a=10)
explicit_b = kw_default_expr_order_4297(b=20)

if omitted != (1, 2)
    error("keyword defaults were not evaluated left-to-right")
end

if explicit_a != (10, 11)
    error("later keyword default did not see explicit earlier keyword")
end

if explicit_b != (1, 20)
    error("explicit later keyword value did not override its default")
end

if kw_default_expr_global_ref_4297() != 42
    error("keyword default expression did not see global binding")
end

if kw_default_expr_global_ref_4297(x=9) != 9
    error("explicit keyword value did not override global default expression")
end

if kw_default_expr_global_symbol_4297() != :default
    error("keyword default expression did not preserve global Symbol binding")
end

if kw_default_expr_global_symbol_4297(debuginfo=:source) != :source
    error("explicit keyword value did not override global Symbol default")
end

if kw_default_expr_zero_arg_call_num_4297() != 42
    error("keyword default expression did not evaluate zero-arg numeric call")
end

if kw_default_expr_zero_arg_call_num_4297(x=9) != 9
    error("explicit keyword value did not override zero-arg numeric call default")
end

if kw_default_expr_zero_arg_call_symbol_4297() != :default
    error("keyword default expression did not evaluate zero-arg Symbol call")
end

if kw_default_expr_zero_arg_call_symbol_4297(debuginfo=:source) != :source
    error("explicit keyword value did not override zero-arg Symbol call default")
end

if kw_default_expr_arg_call_num_4297() != 42
    error("keyword default expression did not evaluate argument numeric call")
end

if kw_default_expr_arg_call_num_4297(x=9) != 9
    error("explicit keyword value did not override argument numeric call default")
end

if kw_default_expr_arg_call_symbol_4297() != :default
    error("keyword default expression did not evaluate argument Symbol call")
end

if kw_default_expr_arg_call_symbol_4297(debuginfo=:source) != :source
    error("explicit keyword value did not override argument Symbol call default")
end

if kw_default_expr_kw_call_num_4297() != 42
    error("keyword default expression did not evaluate keyword numeric call")
end

if kw_default_expr_kw_call_num_4297(x=9) != 9
    error("explicit keyword value did not override keyword numeric call default")
end

if kw_default_expr_kw_call_dep_4297() != 42
    error("keyword default expression keyword call did not see earlier default")
end

if kw_default_expr_kw_call_dep_4297(seed=10) != 11
    error("keyword default expression keyword call did not see explicit earlier keyword")
end

if kw_default_expr_kw_call_dep_4297(x=9) != 9
    error("explicit keyword value did not override dependent keyword call default")
end

if kw_default_expr_kw_call_symbol_4297() != :default
    error("keyword default expression did not evaluate keyword Symbol call")
end

if kw_default_expr_kw_call_symbol_4297(debuginfo=:source) != :source
    error("explicit keyword value did not override keyword Symbol call default")
end

if Base.infer_return_type(kw_default_expr_arg_call_num_4297, Tuple{}) != Int64
    error("keyword default expression argument call return type was not inferred as Int64")
end

if Base.infer_return_type(kw_default_expr_arg_call_symbol_4297, Tuple{}) != Symbol
    error("keyword default expression argument call return type was not inferred as Symbol")
end

if Base.infer_return_type(kw_default_expr_kw_call_num_4297, Tuple{}) != Int64
    error("keyword default expression keyword call return type was not inferred as Int64")
end

if Base.infer_return_type(kw_default_expr_kw_call_symbol_4297, Tuple{}) != Symbol
    error("keyword default expression keyword call return type was not inferred as Symbol")
end

kw_default_expr_symbol_4297 = :none

if kw_default_expr_global_symbol_4297() != :none
    error("keyword default expression captured global binding at definition time")
end

true
