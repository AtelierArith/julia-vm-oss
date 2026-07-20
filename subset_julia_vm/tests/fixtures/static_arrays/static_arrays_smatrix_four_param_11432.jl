using StaticArrays

# Issue #11432 MWE: a struct field annotated with the upstream canonical
# four-parameter SMatrix{M,N,T,L} form used to raise "too many parameters for
# type StaticArrays.SMatrix" once synthetic default-constructor validation
# became Julia-compatible (PR #11358), because the bundled struct only
# declared SMatrix{M,N,T}.
struct AffineSMatrixArityGap
    W::SMatrix{2,2,Float64,4}
end

a = AffineSMatrixArityGap(@SMatrix [1.0 0.0; 0.0 1.0])

# Partial parameterization stays constructible (upstream models SMatrix as the
# alias SMatrix{S1,S2,T,L} = SArray{Tuple{S1,S2},T,2,L}; the bundled struct
# keeps SMatrix{M,N,T} as the incomplete `SMatrix{M,N,T,L} where L`).
m_full = SMatrix{2,2,Float64,4}((1.0, 2.0, 3.0, 4.0))
m_3    = SMatrix{2,2,Float64}(1.0, 2.0, 3.0, 4.0)
m_2    = SMatrix{2,2}(1.0, 2.0, 3.0, 4.0)
m_mac  = @SMatrix [1.0 3.0; 2.0 4.0]

# A user function specialized on the narrower SMatrix{N,N,T} form (dropping L)
# must still dispatch on the fully-parameterized value (Issue #8090-style).
narrow(x::SMatrix{N,N,T}) where {N,T} = (N, T)

# Matrix-vector product (arraymath.jl fast path) must still report the
# correct four-parameter result type, not a stale three-parameter name.
Wv = SMatrix{2,2}(1.0, 0.0, 0.0, 1.0) * SVector(3.0, 4.0)

ok = typeof(a.W) == typeof(m_full) &&
     a.W isa SMatrix{2,2,Float64,4} &&
     a.W isa SMatrix{2,2,Float64} &&
     a.W isa SMatrix{2,2} &&
     m_full == m_3 == m_2 == m_mac &&
     typeof(m_full) == typeof(m_3) == typeof(m_2) == typeof(m_mac) &&
     narrow(m_full) == (2, Float64) &&
     Wv isa SVector{2,Float64} &&
     Wv[1] == 3.0 && Wv[2] == 4.0 &&
     typeof(a) == AffineSMatrixArityGap

# Constructing with a mismatched flat length must still error (upstream:
# DimensionMismatch), not silently wrap around or corrupt the shape.
bad = try
    SMatrix{2,2,Float64,5}(1.0, 2.0, 3.0, 4.0, 5.0)
    false
catch
    true
end

println((typeof(a.W), typeof(Wv), ok, bad))
ok && bad
