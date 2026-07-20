using Test

# Issues #11179 / #11193: a struct-body `global` helper (Issue #11005) is an
# ORDINARY global method whose body just happens to retain privileged `new`.
# Its body must therefore be lowered through the central LambdaContext routing
# authority (`function_lowering_capabilities`, Issues #10936/#10965), exactly
# like every other function-definition entry surface.
#
# The privileged `new` and the LambdaContext are ORTHOGONAL: `new` authority
# comes from `Function.new_struct_name` (a compile-time flag), NOT from being
# lowered context-free. An earlier fix for #11179 (commit d0dfe0578) kept `new`
# working by lowering these helpers context-FREE — which silently denied them
# the enclosing macro tables, so a user-defined macro call inside a helper body
# could not be lowered AT ALL (`UnsupportedFeature { kind: MacroCall }`), while
# every audit stayed green.
#
# `global_new_helper_11005.jl` already covers `where` binders, `new{T}`, lifted
# closures (#11188), begin/let/@kwdef (#11186) and splat (#11187). This fixture
# locks the remaining uncovered capability of the routing authority: MACRO
# EXPANSION inside a struct-body `global` helper body.

macro double_it11179(x)
    return :($(esc(x)) * 2)
end

# --- short form ---

struct MacroHelper11179
    v::Int
    global macro_helper11179(x) = new(@double_it11179(x))
end

@test macro_helper11179(5).v == 10
@test macro_helper11179(0).v == 0

# --- long form (`global function ... end`) ---

struct MacroHelperLong11179
    v::Int
    global function macro_helper_long11179(x)
        doubled = @double_it11179(x)
        new(doubled)
    end
end

@test macro_helper_long11179(7).v == 14

# --- macro call AND a `where` binder in the same helper body ---
# Both capabilities of the routing authority at once: macro expansion needs the
# context's macro tables, the `where` binder needs its lexical binder state.

struct MacroWhereHelper11179{T}
    x::T
    global macro_where_helper11179(::Type{T}, x) where {T} = new{T}(@double_it11179(x))
end

@test typeof(macro_where_helper11179(Int, 4)) === MacroWhereHelper11179{Int}
@test macro_where_helper11179(Int, 4).x == 8
@test typeof(macro_where_helper11179(Float64, 1.5)) === MacroWhereHelper11179{Float64}
@test macro_where_helper11179(Float64, 1.5).x == 3.0

true
