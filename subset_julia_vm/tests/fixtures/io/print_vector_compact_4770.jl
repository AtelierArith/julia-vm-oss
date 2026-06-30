# Issue #4770: print(::Vector) / println(::Vector) / string(::Vector)
# leaked the internal Pure-Julia Array{T} wrapper struct form
# (`Array{Int64}(3-element Memory{Int64}:\n 2\n 4\n 6, (3))`) instead
# of the compact `[2, 4, 6]` form. Sibling of #4731 which fixed
# `repr(::Vector)`; same root cause family but for the print path.
#
# Fix: add an Array-wrapper arm to `format_struct_instance` in
# `subset_julia_vm/src/vm/formatting.rs` that walks the `_mem`
# Memory carrier and emits the compact `[a, b, c]` (1D) / `[a b; c d]`
# (2D) / `T[]` (empty) form, mirroring the Pure-Julia
# `_show_vector_compact` / `_show_matrix_compact` helpers in
# `julia/base/io.jl`.

using Test

@testset "print(::Vector) compact form (Issue #4770)" begin
    v = [2, 4, 6]
    @test string(v) == "[2, 4, 6]"

    buf = IOBuffer()
    print(buf, v)
    @test String(take!(buf)) == "[2, 4, 6]"
end

@testset "string(::Vector{String}) uses show form for elements (Issue #4770)" begin
    v = ["a", "b", "c"]
    # String elements are quoted (show form)
    @test string(v) == "[\"a\", \"b\", \"c\"]"
end

@testset "string(typed empty Vector) preserves eltype (Issue #4770)" begin
    @test string(Int64[]) == "Int64[]"
    @test string(String[]) == "String[]"
end

@testset "print(::Matrix) compact form (Issue #4770)" begin
    m = [1 2; 3 4]
    @test string(m) == "[1 2; 3 4]"

    buf = IOBuffer()
    print(buf, m)
    @test String(take!(buf)) == "[1 2; 3 4]"
end

@testset "print(::Vector) from sort/filter result (Issue #4770)" begin
    # These functions return Pure-Julia Array wrappers that previously
    # leaked the Memory carrier.
    @test string(sort([3, 1, 2])) == "[1, 2, 3]"
    @test string(filter(iseven, 1:6)) == "[2, 4, 6]"
end

true
