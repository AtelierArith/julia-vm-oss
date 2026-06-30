# Utility helpers shared by the rotation types (adapted from
# extern/Rotations.jl/src/util.jl).
#
# The 3-D `skew` / `perpendicular_vector` helpers are introduced in Phase 3
# alongside the 3-D rotation types that use them.

@inline function check_length(v, len)
    if length(v) != len
        throw(DimensionMismatch("Expected length $(len), got length $(length(v))"))
    end
end

# The element type for a rotation matrix built from trig functions of an angle.
rot_eltype(::Type{T}) where {T} = typeof(sin(zero(T)))
rot_eltype(::Type{T}) where {T<:AbstractFloat} = T

"""
    skew(v) -> SMatrix{3,3}

The 3×3 skew-symmetric (cross-product) matrix of a length-3 vector `v`:
`[0 -v₃ v₂; v₃ 0 -v₁; -v₂ v₁ 0]`. Written with an explicit column-major flat
tuple (no `@SMatrix` comprehension, which does not lower inside a
bundled-package include).
"""
function skew(v)
    check_length(v, 3)
    v1 = v[1]; v2 = v[2]; v3 = v[3]
    z = zero(v1)
    # column-major: col1=(0,v3,-v2), col2=(-v3,0,v1), col3=(v2,-v1,0)
    return SMatrix{3,3}((z, v3, -v2, -v3, z, v1, v2, -v1, z))
end

"""
    isrotationgenerator(m) -> Bool

`true` when the (static) matrix `m` is skew-symmetric (`m == -mᵀ`).
"""
function isrotationgenerator(m)
    s = size(m)
    s[1] == s[2] || return false
    n = s[1]
    for j in 1:n
        for i in 1:n
            m[i, j] == -m[j, i] || return false
        end
    end
    return true
end
