module Symbolics

include("types.jl")
include("arithmetic.jl")
include("linear_algebra.jl")
include("show.jl")
include("substitute.jl")
include("simplify.jl")
include("diff.jl")
include("variables.jl")

export Num, Sym, Term, @variables, value, unwrap
export operation, arguments, iscall, issym, isterm
export substitute, simplify, expand
export Differential, derivative, expand_derivatives

end
