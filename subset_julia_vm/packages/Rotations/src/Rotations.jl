module Rotations

# Rotations.jl support for SubsetJuliaVM (parent Issue #7434).
#
# Pure-Julia, upstream-shaped MVP adapted to the sjulia subset.  Mirrors the
# upstream `extern/Rotations.jl/src/` include layout, but hand-expands the
# upstream `@eval`/`@generated`/`Base.@pure` constructs that exceed current
# macro/lowering support and adapts to sjulia's 3-parameter `SMatrix{M,N,T}`
# (no length parameter `L`).  Numeric output matches upstream Rotations 1.7.1
# for the MVP fixtures.  Deferred surface (ForwardDiff derivatives, RecipesBase
# plotting, Unitful, exhaustive Euler-order variants, eigen/decomposition
# parity) is documented in docs/vm/ROTATIONS.md.

using LinearAlgebra
using StaticArrays
using Quaternions

include("util.jl")
include("core_types.jl")
include("euler_types.jl")
include("angleaxis_types.jl")
include("quaternion_types.jl")
include("param3_types.jl")
include("rotation_between.jl")
include("generator_types.jl")

# Rotation types
export Rotation, RotMatrix, RotMatrix2, RotMatrix3
export Angle2d
export RotX, RotY, RotZ
export AngleAxis
export QuatRotation, RotationVec, RodriguesParam, MRP
# rotation generators
export RotationGenerator, Angle2dGenerator, RotationVecGenerator
# interface (upstream does NOT export `params`; it is `Rotations.params`)
export rotation_angle, rotation_axis, rotation_between, isrotation
export isrotationgenerator

end # module Rotations
