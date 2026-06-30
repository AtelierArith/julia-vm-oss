include("../../../packages/MacroTools/src/match/match.jl")
include("../../../packages/MacroTools/src/match/types.jl")
include("../../../packages/MacroTools/src/match/union.jl")

assoc!(d, k, v) = (d[k] = v; d)
isexpr(x::Expr) = true
isexpr(x) = false
isexpr(x::Expr, ts...) = x.head in ts
isexpr(x, ts...) = false
isline(ex) = isexpr(ex, :line) || isa(ex, LineNumberNode)
rmlines(x) = x
rmlines(x::Expr) = x
unblock(ex) = ex

include("../../../packages/MacroTools/src/match/macro.jl")

function splitarg_like(arg_expr3)
    @match arg_expr3 begin
        (arg_name_Symbol::arg_type_) => (arg_name, arg_type)
        (arg_name_Symbol) => (arg_name, nothing)
        (::arg_type_) => (nothing, arg_type)
        _ => error("Invalid")
    end
end

true
