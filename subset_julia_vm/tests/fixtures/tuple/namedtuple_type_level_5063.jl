# Issue #5063: type-level `NamedTuple{names, T}` support — subtype, isa,
# dispatch, and construction on the parametric NamedTuple type. All assertions
# match upstream Julia 1.12.

checks = Bool[]

# --- isa against the parametric NamedTuple type ---------------------------
# Names-only form `NamedTuple{(:a, :b)}` matches any field types but requires
# the exact field names in order.
push!(checks, (a = 1, b = 2) isa NamedTuple{(:a, :b)})
push!(checks, (a = 1.0, b = "x") isa NamedTuple{(:a, :b)})
# Arity and order of the field names are significant.
push!(checks, !((a = 1, b = 2, c = 3) isa NamedTuple{(:a, :b)}))
push!(checks, !((b = 1, a = 2) isa NamedTuple{(:a, :b)}))
# Single-field marker carries a trailing comma.
push!(checks, (x = 1,) isa NamedTuple{(:x,)})

# Names + field-type tuple form `NamedTuple{(:a, :b), Tuple{Int, Int}}`.
push!(checks, (a = 1, b = 2) isa NamedTuple{(:a, :b),Tuple{Int,Int}})
push!(checks, (a = 1, b = 2.0) isa NamedTuple{(:a, :b),Tuple{Int,Float64}})
push!(checks, !((a = 1, b = 2.0) isa NamedTuple{(:a, :b),Tuple{Int,Int}}))

# Every named tuple is an instance of the bare `NamedTuple`.
push!(checks, (a = 1, b = 2) isa NamedTuple)

# --- typeof / === --------------------------------------------------------
# The parametric spelling canonicalizes to the same object as `typeof`.
push!(checks, typeof((a = 1, b = 2)) === NamedTuple{(:a, :b),Tuple{Int64,Int64}})
push!(checks, typeof((x = 1,)) === NamedTuple{(:x,),Tuple{Int64}})

# --- subtype (<:) --------------------------------------------------------
push!(checks, NamedTuple{(:a, :b),Tuple{Int,Int}} <: NamedTuple)
push!(checks, NamedTuple{(:a, :b)} <: NamedTuple)
push!(checks, NamedTuple{(:a, :b),Tuple{Int,Int}} <: NamedTuple{(:a, :b)})

# --- dispatch ------------------------------------------------------------
f(::NamedTuple{(:a, :b)}) = "ab"
f(::NamedTuple) = "any"
push!(checks, f((a = 1, b = 2)) == "ab")
push!(checks, f((x = 1,)) == "any")

g(::NamedTuple{(:a, :b),Tuple{Int,Int}}) = "abii"
g(::NamedTuple) = "any"
push!(checks, g((a = 1, b = 2)) == "abii")
push!(checks, g((a = 1.0, b = 2.0)) == "any")

# Dispatch through a variable binding (not only an inline literal).
nt = (a = 1, b = 2)
push!(checks, f(nt) == "ab")

# Most-specific method among several field-name patterns is selected.
h(::NamedTuple{(:x, :y)}) = "xy"
h(::NamedTuple{(:a, :b)}) = "ab"
h(::NamedTuple) = "nt"
push!(checks, h((x = 1, y = 2)) == "xy")
push!(checks, h((a = 1, b = 2)) == "ab")
push!(checks, h((p = 1,)) == "nt")

# --- construction --------------------------------------------------------
# Positional tuple is mapped to the field names; the typed form converts each
# element to its declared field type.
push!(checks, NamedTuple{(:a, :b)}((1, 2)) == (a = 1, b = 2))
push!(checks, NamedTuple{(:a, :b),Tuple{Int,Int}}((1, 2)) == (a = 1, b = 2))
let v = NamedTuple{(:a, :b),Tuple{Float64,Int}}((1, 2))
    push!(checks, v.a === 1.0 && v.b === 2)
    push!(checks, typeof(v) === NamedTuple{(:a, :b),Tuple{Float64,Int64}})
end
# A named-tuple argument with matching field names also constructs.
push!(checks, NamedTuple{(:a, :b)}((a = 1, b = 2)) == (a = 1, b = 2))

all(checks)
