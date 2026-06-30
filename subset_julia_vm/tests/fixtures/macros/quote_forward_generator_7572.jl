f = :forward_target_7572
T = :ForwardBox7572
field = :value
fs = [f]

ex = :($([:($f(x::$T, args...; kwargs...) =
           (Base.@_inline_meta; $f(x.$field, args...; kwargs...)))
         for f in fs]...);
      nothing)

ex isa Expr &&
    ex.head === :block &&
    length(ex.args) == 4 &&
    ex.args[2].head === :(=)
