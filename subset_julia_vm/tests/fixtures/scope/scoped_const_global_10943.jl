# Issue #10943: `const global c = 1` and `global const c = 1` parse to
# ConstDeclaration(GlobalDeclaration(...)) but lowering silently dropped the
# nested binding, leaving `c` undefined. Both modifier orders must lower to a
# real const global binding.

const global scope_const_global_10943 = 1
@assert scope_const_global_10943 == 1

global const scope_global_const_10943 = 2
@assert scope_global_const_10943 == 2

# Typed declared name.
global const scope_global_const_typed_10943::Int = 3
@assert scope_global_const_typed_10943 == 3

# Newline between the modifiers (upstream accepts `global\nconst c = 1`).
global
const scope_global_nl_const_10943 = 4
@assert scope_global_nl_const_10943 == 4

# Structured type alias through the scoped wrapper stays a registered alias.
const global ScopeIntVec10943 = Vector{Int}
@assert ScopeIntVec10943 == Vector{Int}
@assert ScopeIntVec10943([1, 2, 3]) == [1, 2, 3]

# The const binding is readable from function scope like any global const.
scope_read_const_10943() = scope_const_global_10943 + scope_global_const_10943
@assert scope_read_const_10943() == 3

true
