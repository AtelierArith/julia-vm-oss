# Issue #4780: `Symbol(parts...)` with multiple arguments failed at
# compile time with `Compilation error: Symbol requires exactly 1
# argument: Symbol(name)`. Upstream Julia's
# `Base.Symbol(args...) = Symbol(string(args...))` concatenates all
# parts via `string` then forms a single Symbol — common idiom for
# programmatically-built symbols like `Symbol("col_", i)`.
#
# Fix: SymbolNew compile arm in subset_julia_vm_compile/src/compile/expr/builtin.rs
# now accepts args.len() >= 1 and emits the full argc into the
# CallBuiltin. The runtime arm in vm/builtins_macro/mod.rs pops argc
# values, formats each via format_value_print, and concatenates.

using Test

@testset "Symbol(name) single-arg form unchanged (Issue #4780)" begin
    @test Symbol("hello") === :hello
    @test Symbol(:already_sym) === :already_sym
end

@testset "Symbol(prefix, n) — common 'col_N' idiom (Issue #4780)" begin
    @test Symbol("col_", 1) === :col_1
    @test Symbol("col_", 42) === :col_42
end

@testset "Symbol(a, b, c) — three string parts (Issue #4780)" begin
    @test Symbol("my", "_", "var") === :my_var
end

@testset "Symbol(:sym, str, :sym) — mixed Symbol and String (Issue #4780)" begin
    @test Symbol(:prefix, "_", :suffix) === :prefix_suffix
end

@testset "Symbol(a, b, c, d, e) — many parts (Issue #4780)" begin
    @test Symbol("a", "b", "c", "d", "e") === :abcde
end

true
