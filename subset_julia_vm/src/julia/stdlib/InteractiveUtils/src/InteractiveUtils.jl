# This file is a part of SubsetJuliaVM. License is MIT.

# =============================================================================
# InteractiveUtils - Utilities for interactive use
# =============================================================================
# Minimal subset of Julia's stdlib/InteractiveUtils
#
# IMPORTANT: Most InteractiveUtils functions require compiler introspection
# (LLVM IR, native code, method tables) which is not available in SubsetJuliaVM.
#
# Functions NOT included (require Julia runtime internals):
# - @code_typed, @code_lowered, @code_llvm, @code_native
# - @which, @edit, @less, @functionloc
# - methodswith, varinfo
# - clipboard, apropos
# - peakflops (requires LinearAlgebra)
#
# Supported here:
# - versioninfo, supertypes
# - code_warntype / @code_warntype (type-stability diagnostic, Issue #5145):
#   builds on the shared inference surface that backs Base.infer_return_type /
#   Core.Compiler.return_type. Output is a subset of upstream's IR dump.
#
# This module provides only the functions that can be meaningfully implemented
# in a subset VM environment.

module InteractiveUtils

export versioninfo, supertypes, code_warntype, @code_warntype

# =============================================================================
# versioninfo - Display version information
# =============================================================================

# versioninfo() - Show SubsetJuliaVM information
# Note: Output differs from Julia's versioninfo() by design, as this is
# a different runtime environment.
#
# In Julia, versioninfo(io::IO=stdout; verbose::Bool=false) displays:
# - Julia version, commit, and build info
# - Platform info (OS, architecture, word size)
# - CPU and thread info
# - LLVM version
# - Environment variables
#
# SubsetJuliaVM provides a subset of this information relevant to our
# bytecode interpreter environment.

"""
    versioninfo()

Print information about the SubsetJuliaVM version.

The output includes version number, platform type, and VM characteristics.
Note that SubsetJuliaVM does not have JIT compilation or LLVM integration.

# Examples
```julia
julia> using InteractiveUtils
julia> versioninfo()
SubsetJuliaVM Version 0.5.4
Platform: Bytecode Interpreter (no JIT)
...
```

See also: [`VERSION`](@ref)
"""
function versioninfo()
    v = VERSION
    version_str = string(v.major, ".", v.minor, ".", v.patch)
    println("SubsetJuliaVM (Rust implementation) Version ", version_str)
    println()
    println("Platform: Bytecode Interpreter (no JIT)")
    println("  Pipeline: Parser → Lowering → Compiler → VM")
    println("  Execution model: deterministic")
    println("  RNG: StableRNG (StableRNGs.jl compatible)")
    println()
    println("Targets: iOS, WebAssembly, CLI")
    println("  App Store compatible: yes (no dynamic code generation)")
    nothing
end

# =============================================================================
# supertypes - Get the supertype chain of a type
# =============================================================================

# supertypes(T::Type) - Return a tuple of T and all its supertypes
# For SubsetJuliaVM's built-in types, we provide the standard Julia type hierarchy.
#
# Note: This is implemented as a stub that works with type names as strings,
# since full type reflection is not available in SubsetJuliaVM.
# The VM handles typeof() and isa() as builtins.
#
# Julia type hierarchy (for reference):
#   Int64 <: Signed <: Integer <: Real <: Number <: Any
#   Float64 <: AbstractFloat <: Real <: Number <: Any
#   Bool <: Integer <: Real <: Number <: Any
#   String <: AbstractString <: Any
#   Array <: AbstractArray <: Any
#
# Since we cannot pass Type objects in SubsetJuliaVM, this function
# is provided as documentation. Use typeof(x) and isa(x, T) for type checks.

# Stub implementation - prints type hierarchy information
function supertypes(typename)
    # Note: In Julia, supertypes takes a Type object.
    # In SubsetJuliaVM, we accept a value and show its type hierarchy.
    t = typeof(typename)
    if t == "Int64"
        println("Int64 <: Signed <: Integer <: Real <: Number <: Any")
    elseif t == "Float64"
        println("Float64 <: AbstractFloat <: Real <: Number <: Any")
    elseif t == "Bool"
        println("Bool <: Integer <: Real <: Number <: Any")
    elseif t == "String"
        println("String <: AbstractString <: Any")
    elseif t == "Array"
        println("Array <: AbstractArray <: Any")
    elseif t == "Nothing"
        println("Nothing <: Any")
    else
        println(t, " <: Any")
    end
end

# =============================================================================
# code_warntype - Type-stability diagnostic (Issue #5145)
# =============================================================================
#
# In official Julia, `code_warntype([io], f, types)` prints the type-inferred
# IR for a method, emphasizing (in red) any slot/return type that is not
# concrete — `Union{...}` or `Any` — so users can spot type instabilities that
# hurt performance.
#
# SubsetJuliaVM does not materialize full compiler IR, so this implementation
# provides the diagnostic surface that the no-JIT runtime can support: it
# reports the matched method signature and the inferred return type, and flags
# whether that return type is concrete (type stable) or non-concrete (a
# `Union`/`Any`, i.e. potentially type unstable). Output is intentionally a
# subset of upstream's IR dump — like `versioninfo`, the printed text differs
# by design — but the return value matches upstream (`nothing`), and the
# return-type query routes through the same shared inference surface that backs
# `Base.infer_return_type` / `Core.Compiler.return_type`.
#
# See also: `Base.infer_return_type`, `Base.return_types`, `Base.code_typed`.

# Whether `rt` should be emphasized as a potential type instability, mirroring
# upstream's "non-concrete return type" warning. Concrete types (e.g. `Int64`)
# are stable; `Union{...}` and `Any` are not.
_code_warntype_is_unstable(rt) = !isconcretetype(rt)

"""
    code_warntype([io::IO], f, types)

Print a type-stability diagnostic for the method of the generic function `f`
matched by the argument tuple type `types`.

The report lists the matched method signature and the inferred return type, and
marks the return type as type stable when it is a concrete type or as a possible
type instability when it is a `Union` or `Any`.

!!! note
    SubsetJuliaVM does not have a JIT or full compiler IR, so the printed output
    is a subset of official Julia's `code_warntype` IR dump. The inferred return
    type matches `Base.infer_return_type` / `Core.Compiler.return_type`.

# Examples
```julia
using InteractiveUtils
f(x::Int64) = x + 1
code_warntype(f, Tuple{Int64})
```

See also: [`@code_warntype`](@ref), [`Base.infer_return_type`](@ref),
[`Base.code_typed`](@ref).
"""
function code_warntype(io::IO, f, types)
    ms = Base.methods(f, types)
    if length(ms) == 0
        println(io, "code_warntype: no method matching the given argument types")
        return nothing
    end
    for m in ms
        # Header: matched method signature (mirrors upstream's "MethodInstance
        # for f(::T...)" line in spirit).
        print(io, "MethodInstance for ", m.name, "(")
        nparams = m.nargs - 1
        for i in 1:nparams
            if i > 1
                print(io, ", ")
            end
            if i <= length(m.sig)
                print(io, "::", m.sig[i])
            else
                print(io, "::Any")
            end
        end
        println(io, ")")
        rt = m.return_type
        if _code_warntype_is_unstable(rt)
            # Upstream emphasizes non-concrete return types in red; the no-color
            # subset annotates them so the instability is still visible.
            println(io, "Body::", rt, "   # type unstable: non-concrete return type")
        else
            println(io, "Body::", rt)
        end
    end
    return nothing
end

code_warntype(f, types) = code_warntype(stdout, f, types)

"""
    @code_warntype f(args...)

Evaluates the arguments of the function call `f(args...)`, determines their
types, and calls [`code_warntype`](@ref) on the resulting signature.

# Examples
```julia
using InteractiveUtils
f(x::Int64) = x + 1
@code_warntype f(1)
```

See also: [`code_warntype`](@ref), [`Base.infer_return_type`](@ref).
"""
macro code_warntype end

end # module InteractiveUtils
