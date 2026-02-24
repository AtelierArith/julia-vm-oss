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
# - @code_typed, @code_lowered, @code_llvm, @code_native, @code_warntype
# - @which, @edit, @less, @functionloc
# - methodswith, subtypes, varinfo
# - clipboard, apropos
# - peakflops (requires LinearAlgebra)
#
# This module provides only the functions that can be meaningfully implemented
# in a subset VM environment.

module InteractiveUtils

export versioninfo, supertypes

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

end # module InteractiveUtils
