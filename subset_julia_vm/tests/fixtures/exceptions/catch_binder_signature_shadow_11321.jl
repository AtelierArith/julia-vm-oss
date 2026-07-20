# A `catch NAME` binder introduces a fresh lexical binding for NAME that
# shadows any outer const/type alias spelled NAME, for the extent of the
# clause — including a NAME used INSIDE a composite annotation like
# `Vector{T}`, not just a bare one. Signature type expressions must resolve
# against the binding visible at the definition's source position, matching
# upstream's eager evaluation of signature annotations (Issue #11321).

# Shape 1: the catch binder shadows an outer const type alias used inside a
# composite annotation. Upstream raises TypeError immediately when the
# method is defined (the caught ErrorException is not a Type), instead of
# silently freezing the annotation to the outer alias's target.
const T = Int64
err = nothing
try
    error("x")
catch T
    try
        f(x::Vector{T}) = 1
    catch e
        global err = e
    end
end
@assert typeof(err) == TypeError "expected TypeError, got $(typeof(err))"

# Shape 2 (related valid case): a same-clause reassignment of the catch
# binder to a resolvable type, BEFORE the method definition, makes that
# target visible to the definition within the SAME clause.
result = nothing
try
    error("y")
catch S
    S = Int64
    g(x::S) = 99
    global result = g(1)
end
@assert result == 99 "expected 99, got $result"

# No-leak control: the catch-scoped shadow/rebinding must not survive past
# the clause's `end` — a definition after the try/catch still sees the outer
# (plain, non-const) global alias, exactly like upstream.
U = Int64
try
    error("z")
catch U
    U = Float64
    h(x::U) = 1
    @assert h(1.0) == 1
end
outer(x::U) = 2
@assert outer(1) == 2 "expected the outer alias to survive unshadowed after the catch clause"

# Shape 3 (non-regression): a name used inside a composite annotation is not
# always a shadowed alias — it can be an ordinary local holding an upstream-
# legal ISBITS type-parameter value. `Vector{7}` is a real `DataType`
# upstream (any `isbits` value, `Symbol`, or `Type`/`TypeVar` is a legal type
# parameter, not just a `Type` itself), so the definition must be accepted
# exactly as it is with no `catch` involved at all — only the call fails,
# with `MethodError` (`Vector{7}` does not match the argument's actual type
# `Vector{Int64}`). An earlier revision of this fix's compile-time probe
# validated a shadowed name with a bare `name <: Any`, which demands the
# runtime value literally BE a Type — that wrongly turned this upstream-legal
# definition into a `TypeError` at definition time.
x = 7
q2(v::Vector{x}) = 1
q2_err = nothing
try
    q2(Int64[7])
catch e
    global q2_err = e
end
@assert q2_err isa MethodError "expected MethodError calling q2([7]) against Vector{7}, got $(typeof(q2_err))"

println("catch_binder_signature_shadow_11321: all assertions passed")
true
