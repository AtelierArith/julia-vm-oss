using StaticArraysCore

# Issue #11542 MWE: a struct field annotated with the upstream canonical
# four-parameter SMatrix{M,N,T,L} form used to raise "too many parameters for
# type StaticArraysCore.SMatrix" under `using StaticArraysCore` directly.
# #11432 fixed the same gap for the separate, independent bundled StaticArrays
# package's own `SMatrix` struct; this covers StaticArraysCore's own copy.
struct AffineSMatrixArityGapCore
    W::SMatrix{2,2,Float64,4}
end

# Only constructs/checks that also run under real upstream StaticArraysCore
# (a minimal core-types package with no vararg/2-param SMatrix constructors
# and no AbstractArray interface methods) are used below, so this fixture is
# parity-clean against upstream `julia` with StaticArraysCore installed.
m_full = SMatrix{2,2,Float64,4}((1.0, 2.0, 3.0, 4.0))
m_3 = SMatrix{2,2,Float64}((1.0, 2.0, 3.0, 4.0))

# A user function specialized on the narrower SMatrix{N,N,T} form (dropping
# L) must still dispatch on the fully-parameterized value.
narrow(x::SMatrix{N,N,T}) where {N,T} = (N, T)

a = AffineSMatrixArityGapCore(SMatrix{2,2,Float64,4}((1.0, 0.0, 0.0, 1.0)))

ok = typeof(a.W) == typeof(m_full) &&
     a.W isa SMatrix{2,2,Float64,4} &&
     a.W isa SMatrix{2,2,Float64} &&
     a.W isa SMatrix{2,2} &&
     typeof(m_full) == typeof(m_3) &&
     narrow(m_full) == (2, Float64) &&
     typeof(a) == AffineSMatrixArityGapCore

# Constructing with a mismatched flat length must still error (upstream:
# ArgumentError), not silently wrap around or corrupt the shape. Uses the
# multi-arg vararg call shape, which unambiguously dispatches to the checked
# constructor; the single-flat-Tuple-argument call shape has a known,
# separately-tracked validation gap (Issue #11573).
bad = try
    SMatrix{2,2,Float64,5}(1.0, 2.0, 3.0, 4.0, 5.0)
    false
catch
    true
end

println((typeof(a.W), ok, bad))
ok && bad
