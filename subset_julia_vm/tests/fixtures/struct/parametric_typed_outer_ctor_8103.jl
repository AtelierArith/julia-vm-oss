# Issue #8103: a typed parametric OUTER constructor `T{P...}(x::Number)` must be
# preferred over the synthesized default field constructor `T{P...}(field)` when
# the argument is not convertible to the field type. Previously the parametric
# path treated any arity-matching call as the default field constructor and
# raised a compile-time "Cannot convert ..." instead of selecting the outer ctor.
struct Boxed8103{N,T}
    v::Vector{T}
end
# distinguishing body (2x on the second element) proves the OUTER ctor ran,
# not the default field constructor.
function Boxed8103{N,T}(x::Number) where {N,T}
    Boxed8103{N,T}([T(x), T(2 * x)])
end

b = Boxed8103{2,Float64}(5.0)
# 1-parameter analogue
struct One8103{T}
    v::Vector{T}
end
One8103{T}(x::Number) where {T} = One8103{T}([T(x), T(3 * x)])
c = One8103{Float64}(4.0)

ok = b.v == [5.0, 10.0] && c.v == [4.0, 12.0]
println(ok)
ok
