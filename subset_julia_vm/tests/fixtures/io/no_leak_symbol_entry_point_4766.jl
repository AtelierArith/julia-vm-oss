# Issue #4766 / #5038 (prevention sibling of #4757 / #4766 IOBuffer matrix /
# #4774 container matrix): the `Symbol(x)` / `Symbol(args...)` display entry
# point (`BuiltinId::SymbolNew`, `subset_julia_vm/src/vm/builtins_macro/mod.rs`)
# fed a raw popped Value to `format_value_print` WITHOUT first resolving
# `Value::StructRef` against the struct heap, so `Symbol(::struct)` /
# `Symbol(::Pair)` leaked the Rust debug `StructRef(heap_idx=N)` repr into the
# symbol name (#5038, discovered while building the #4766 audit script
# scripts/check_format_value_resolves_structref.sh).
#
# This fixture guards the Symbol stringify entry point — both the 1-arg
# `Symbol(struct)` fast/fallback path and the multi-arg
# `Symbol("a_", struct, "_b")` concatenation path — and confirms the symbol
# round-trips cleanly back through the other display entry points
# (`string` / `repr` / interpolation).
#
# Runtime-agnostic: upstream Julia renders `Symbol(SymMatrixFoo4766(1 => 2))`
# without any of the bad tokens either, so the fixture passes under both
# sjulia and julia.

using Test

struct SymMatrixFoo4766
    p::Pair{Int, Int}
end

function no_debug_leak_sym_4766(s)
    bad_tokens = ("StructRef(", "heap_idx=",
                  "I8(", "I16(", "I32(", "I64(", "I128(",
                  "U8(", "U16(", "U32(", "U64(", "U128(",
                  "F16(", "F32(", "F64(",
                  "Str(", "Char(")
    for tok in bad_tokens
        if occursin(tok, s)
            return false
        end
    end
    return true
end

@testset "Symbol(::struct) / Symbol(::Pair) does not leak (Issue #4766/#5038)" begin
    f = SymMatrixFoo4766(Pair(1, 2))
    # 1-arg fallback path. `string(::Symbol)` exercises the symbol-name text.
    @test no_debug_leak_sym_4766(string(Symbol(f)))
    @test no_debug_leak_sym_4766(string(Symbol(Pair(3, 4))))
    # multi-arg concatenation path
    @test no_debug_leak_sym_4766(string(Symbol("a_", f, "_b")))
    @test no_debug_leak_sym_4766(string(Symbol(Pair(5, 6), "_", Pair(7, 8))))
end

@testset "Symbol-of-struct round-trips through string/repr/interp (Issue #4766/#5038)" begin
    f = SymMatrixFoo4766(Pair(1, 2))
    s = Symbol(f)
    @test no_debug_leak_sym_4766(string(s))
    @test no_debug_leak_sym_4766(repr(s))
    @test no_debug_leak_sym_4766("interp: $s")
end

true
