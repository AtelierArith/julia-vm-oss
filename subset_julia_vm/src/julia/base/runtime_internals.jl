# Runtime introspection functions
# Corresponds to julia/base/runtime_internals.jl

# Note: isexported and ispublic are implemented as builtin functions
# and recognized by the compiler. No Julia wrapper needed - they compile
# directly to CallBuiltin instructions.
