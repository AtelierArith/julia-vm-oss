using StaticArrays

# Issue #8537: an inline-constructed parametric struct value is typed at compile
# time only by its bare family name (e.g. `Struct("SVector")`); its value type
# parameters (the `N` in `SVector{N,T}`) are unknown until runtime. Such an
# argument must reach the parameter-generic method via runtime dispatch instead
# of statically binding to a concrete value-parameter sibling like
# `svclass(::SVector{0,T})` (which the bare family loosely matches and would win
# on specificity, or match nothing and raise a spurious MethodError).

svclass(a::SVector{0,T}) where {T} = :zero
svclass(a::SVector{N,T}) where {N,T} = :generic

# a distinct concrete + generic pair: the concrete N=1 method must not capture
# an inline SVector of a different length.
svpick(a::SVector{1,T}) where {T} = :one
svpick(a::SVector{N,T}) where {N,T} = :generic

# plain user parametric struct with a factory (no StaticArrays machinery), to
# show the fix is general rather than StaticArrays-specific.
struct Bag8537{N,T}
    data::Tuple
end
Bag8537(xs...) = Bag8537{length(xs),typeof(xs[1])}(xs)
bagclass(a::Bag8537{0,T}) where {T} = :zero
bagclass(a::Bag8537{N,T}) where {N,T} = :generic

function bare_parametric_value_param_dispatch_8537()
    # inline-constructed SVector must reach the generic method
    ok1 = svclass(SVector(1.0, 2.0)) == :generic
    ok2 = svclass(SVector(1.0, 2.0, 3.0)) == :generic
    # the concrete {0,T} method still wins for a real 0-d value
    ok3 = svclass(SVector{0,Float64}(())) == :zero
    # inline N=2 must not capture the {1,T} concrete method
    ok4 = svpick(SVector(1.0, 2.0)) == :generic
    # a real N=1 value still selects the concrete {1,T} method
    ok5 = svpick(SVector(9.0)) == :one
    # order independence: a 0-d call first must not poison a later fresh N
    _ = svclass(SVector{0,Float64}(()))
    ok6 = svclass(SVector(4.0, 5.0)) == :generic
    # plain parametric struct built through a factory
    ok7 = bagclass(Bag8537(1.0, 2.0)) == :generic
    ok8 = bagclass(Bag8537{0,Float64}(())) == :zero

    return ok1 && ok2 && ok3 && ok4 && ok5 && ok6 && ok7 && ok8
end

bare_parametric_value_param_dispatch_8537()
