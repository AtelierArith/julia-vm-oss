# Issue #4772: regression from PR #4771. The new compact `[a, b, c]`
# form for `print(::Vector)` walked native-array elements (and Memory
# elements) via `format_value` without first resolving
# `Value::StructRef(idx)` against the struct heap. For a Vector of
# heap-allocated structs (any user-defined struct, Pair, etc.), each
# element leaked as `StructRef(heap_idx=N)` inside the compact form.
#
# Fix: extend `resolve_struct_refs_for_format` in
# `subset_julia_vm/src/vm/formatting.rs` to recurse into both
# `Value::Memory` and `Value::ExprArgs`, replacing the carrier
# with a fresh `Any`-typed copy whose elements are pre-resolved.
# The widening is throw-away (only used by the formatting copy of
# the value tree).
#
# Runtime-agnostic guard: assert no `StructRef(` / `heap_idx=` tokens
# leak in any of `string` / `repr` / `print(buf, x)` for a Vector
# whose elements are heap-allocated structs.

using Test

struct VectorElem4772
    x::Int64
    y::Int64
end

@testset "no StructRef leak in print(::Vector{user_struct}) (Issue #4772)" begin
    v = [VectorElem4772(1, 2), VectorElem4772(3, 4)]
    s1 = string(v)
    s2 = repr(v)
    buf = IOBuffer()
    print(buf, v)
    s3 = String(take!(buf))

    for s in (s1, s2, s3)
        @test !occursin("StructRef", s)
        @test !occursin("heap_idx", s)
        # Sanity: the element values must appear (so we know the
        # struct was actually resolved, not just hidden).
        @test occursin("VectorElem4772(1, 2)", s)
        @test occursin("VectorElem4772(3, 4)", s)
    end
end

@testset "no StructRef leak in print(::Vector{Pair}) (Issue #4772)" begin
    vp = [Pair(1, 2), Pair(3, 4)]
    s = string(vp)
    @test !occursin("StructRef", s)
    @test occursin("1 => 2", s)
    @test occursin("3 => 4", s)
end

@testset "no StructRef leak inside Any-typed Vector with struct elements (Issue #4772)" begin
    va = Any[VectorElem4772(7, 8), 42, "hi"]
    s = string(va)
    @test !occursin("StructRef", s)
    @test !occursin("heap_idx", s)
    @test occursin("VectorElem4772(7, 8)", s)
    @test occursin("42", s)
    @test occursin("\"hi\"", s)
end

true
