@doc raw"""
AbstractAlgebra is a pure Julia package for computational abstract algebra.

Repository: <https://github.com/Nemocas/AbstractAlgebra.jl>
"""
module AbstractAlgebra

using LinearAlgebra
using MacroTools
using PrecompileTools
using Preferences
using Random
using RandomExtensions
using SparseArrays

include("imports.jl")

const import_exclude = [
    :import_exclude,
    :QQ,
    :ZZ,
    :RealField,
    :GF,
    :AbstractAlgebra,
    :inv,
    :log,
    :exp,
    :sqrt,
    :div,
    :divrem,
    :numerator,
    :denominator,
    :promote_rule,
    :Set,
    :Module,
    :Group,
]

include("exports.jl")
include("AliasMacro.jl")
include("Aliases.jl")
include("Assertions.jl")
include("Attributes.jl")
include("AbstractTypes.jl")

const PolynomialElem{T} = Union{PolyRingElem{T}, NCPolyRingElem{T}}
const MatrixElem{T} = Union{MatElem{T}, MatRingElem{T}}

include("julia/JuliaTypes.jl")
include("ConcreteTypes.jl")
include("fundamental_interface.jl")
include("KnownProperties.jl")
include("error.jl")
include("julia/Integer.jl")
include("julia/Rational.jl")
include("Poly.jl")
include("FractionResidue.jl")
include("Matrix.jl")
include("Module.jl")
include("Map.jl")
include("PermGroups.jl")
include("YoungTabs.jl")
include("Generic.jl")

const ZZ = JuliaZZ
const QQ = JuliaQQ

end # module AbstractAlgebra
