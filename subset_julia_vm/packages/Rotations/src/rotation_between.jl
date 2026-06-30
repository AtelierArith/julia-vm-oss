# rotation_between(u, v): the rotation taking u onto v along the shortest geodesic
# (adapted from extern/Rotations.jl/src/rotation_between.jl).
#
# Upstream splits this across separate StaticVector{2}/{3}/{N} methods. sjulia
# mis-dispatches sibling concrete methods that differ only in argument type
# (#7960), so — like every other multi-type rotation operation — this is ONE
# method branching on `length` at run time. The general N-D method (which needs
# an SVD) is out of scope for the MVP; 2-D and 3-D are implemented.

# A unit-magnitude vector perpendicular to (x,y,z), matching upstream
# `perpendicular_vector`: swap the two largest-magnitude components, negate the
# second, zero the smallest. Hand-written (no `@SVector` comprehension, which
# does not lower inside a bundled-package include).
function _perpendicular_vector3(x, y, z)
    ax = abs(x); ay = abs(y); az = abs(z)
    z0 = zero(x)
    if ax >= ay && ax >= az
        return ay >= az ? SVector(-y, x, z0) : SVector(-z, z0, x)
    elseif ay >= ax && ay >= az
        return ax >= az ? SVector(y, -x, z0) : SVector(z0, -z, y)
    else
        return ax >= ay ? SVector(z, z0, -x) : SVector(z0, z, -y)
    end
end

"""
    rotation_between(u, v)

The rotation that aligns vector `u` with vector `v` along the shortest path.
Returns an `Angle2d` for 2-D inputs and a `QuatRotation` for 3-D inputs.
"""
function rotation_between(u, v)
    n = length(u)
    n == length(v) || throw(DimensionMismatch("rotation_between: u and v must have equal length"))
    if n == 2
        # angle(complex(v)/complex(u)) == atan(u×v, u·v) (signed, in (-π, π]).
        cross2d = u[1] * v[2] - u[2] * v[1]
        dot2d = u[1] * v[1] + u[2] * v[2]
        (iszero(cross2d) && iszero(dot2d)) &&
            throw(ArgumentError("Input vectors must be nonzero and finite."))
        return Angle2d(atan(cross2d, dot2d))
    elseif n == 3
        du = u[1] * u[1] + u[2] * u[2] + u[3] * u[3]
        dv = v[1] * v[1] + v[2] * v[2] + v[3] * v[3]
        normprod = sqrt(du * dv)
        T = typeof(normprod)
        normprod < eps(T) && throw(ArgumentError("Input vectors must be nonzero."))
        uv = u[1] * v[1] + u[2] * v[2] + u[3] * v[3]
        w = normprod + uv
        if abs(w) < 100 * eps(T)
            # u and v are antiparallel: rotate by π about any perpendicular axis.
            p = _perpendicular_vector3(u[1], u[2], u[3])
            return QuatRotation(w, p[1], p[2], p[3])  # constructor normalises
        end
        cx = u[2] * v[3] - u[3] * v[2]
        cy = u[3] * v[1] - u[1] * v[3]
        cz = u[1] * v[2] - u[2] * v[1]
        return QuatRotation(w, cx, cy, cz)            # constructor normalises
    end
    error("rotation_between supports 2-D and 3-D vectors in this MVP")
end
