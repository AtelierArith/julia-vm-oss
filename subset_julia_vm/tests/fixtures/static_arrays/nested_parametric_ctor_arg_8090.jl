# Issue #8090: a parametric constructor result passed directly (nested, not bound
# to a local) as an argument must dispatch identically to binding it to a local
# first. `SMatrix{2,2}(...)` writes only 2 of the 3 declared `SMatrix{M,N,T}`
# parameters; the runtime value is fully concrete (`SMatrix{2,2,Float64}`), so a
# method specialized on the full parameter set must still be selected.
using StaticArrays

struct Wrap8090{N,T}
    m::SMatrix{N,N,T}
end
Wrap8090(m::SMatrix{N,N,T}) where {N,T} = Wrap8090{N,T}(m)

# (a) nested: constructor result passed directly as the argument.
w_nested = Wrap8090(SMatrix{2,2}((1.0, 2.0, 3.0, 4.0)))
# (b) bound: same value via a local first (this already worked before the fix).
m22 = SMatrix{2,2}((1.0, 2.0, 3.0, 4.0))
w_bound = Wrap8090(m22)

# Generality: different rank and element type, nested vs bound.
w3_nested = Wrap8090(SMatrix{3,3}((1, 2, 3, 4, 5, 6, 7, 8, 9)))
m33 = SMatrix{3,3}((1, 2, 3, 4, 5, 6, 7, 8, 9))
w3_bound = Wrap8090(m33)

ok = typeof(w_nested) === Wrap8090{2,Float64} &&
     typeof(w_bound) === Wrap8090{2,Float64} &&
     typeof(w_nested) === typeof(w_bound) &&
     w_nested isa Wrap8090{2,Float64} &&
     typeof(w3_nested) === Wrap8090{3,Int64} &&
     typeof(w3_nested) === typeof(w3_bound) &&
     w3_nested isa Wrap8090{3,Int64}

println((typeof(w_nested), typeof(w3_nested), ok))
ok
