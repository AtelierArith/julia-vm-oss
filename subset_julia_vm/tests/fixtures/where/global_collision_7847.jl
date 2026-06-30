# Issue #7847: a method `where T` type parameter referenced inside a parametric
# type (`Tuple{T, Int64}`) must shadow a same-named top-level binding
# (`T = Int64`, which sjulia registers as a non-parametric type alias).
#
# Before the fix, the global froze the `x::T` parameter annotation to the alias
# target (`Int64`) at lowering time, so the method's where parameter `T` was
# never bound in the frame's type_bindings and `Tuple{T, Int64}` raised
# `UndefVarError: Unbound type parameter: T` instead of constructing
# `Tuple{Int64, Int64}`. The where parameter lexically shadows the global,
# exactly as upstream Julia scopes it.

# Long-form `function ... where T ... end` with a colliding global.
T = Int64
function tuple_type_for(x::T) where T
    Tuple{T, Int64}
end

# Short-form `f(x::S) where {S<:Real}` with a colliding global and a bound.
S = Float64
g(x::S) where {S<:Real} = Tuple{S, S}

# A where parameter still binds to the concrete argument type, not the global.
r1 = string(tuple_type_for(1)) == "Tuple{Int64, Int64}"
r2 = string(tuple_type_for(2.5)) == "Tuple{Float64, Int64}"
r3 = string(g(1)) == "Tuple{Int64, Int64}"
r4 = string(g(2.5)) == "Tuple{Float64, Float64}"

# A same-named global used as a plain annotation (no `where`) still works as an
# alias, preserving Issue #7840 behavior.
U = Int64
h(x::U) = x + 1
r5 = h(41) == 42

r1 && r2 && r3 && r4 && r5
