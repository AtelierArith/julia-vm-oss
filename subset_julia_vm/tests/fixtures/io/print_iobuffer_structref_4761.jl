# Issue #4761: `print(buf::IOBuffer, x::StructRef)` and
# `print(stdout, x::StructRef)` (multi-arg `print`) leaked the Rust debug
# repr `StructRef(heap_idx=N)` because the IOPrint handler formatted the
# raw stack value without resolving the StructRef against the struct
# heap first (only the `BuiltinId::Print` arm for single-arg `print(x)`
# did that).
#
# This fixture covers the easy slice: the StructRef-resolution path
# for `print(io, ...)`. The broader "route single-arg print through
# user-defined `show(io, ::T)`" slice is now also covered (see
# `print_string_user_show_4761.jl`); multi-arg `print(io, x, y, ...)`
# still defers to the per-value Rust formatter and is tracked
# separately.

using Test

struct PrintBufFoo4761
    x::Int64
    y::Int64
end

@testset "no StructRef leak in print(io, struct) (Issue #4761)" begin
    # Heap-allocated Pair via direct constructor
    p = Pair(1, 2)
    buf = IOBuffer()
    print(buf, p)
    s = String(take!(buf))
    @test !occursin("StructRef", s)
    @test !occursin("heap_idx", s)
    @test s == "1 => 2"
end

@testset "no StructRef leak in print(io, user_struct) (Issue #4761)" begin
    f = PrintBufFoo4761(7, 9)
    buf = IOBuffer()
    print(buf, f)
    s = String(take!(buf))
    @test !occursin("StructRef", s)
    @test !occursin("heap_idx", s)
    # Generic struct format: StructName(field1, field2, ...)
    @test s == "PrintBufFoo4761(7, 9)"
end

@testset "no StructRef leak in print(io, x) for multi-arg (Issue #4761)" begin
    p = Pair(10, 20)
    buf = IOBuffer()
    print(buf, "[", p, "]")
    s = String(take!(buf))
    @test !occursin("StructRef", s)
    @test s == "[10 => 20]"
end

true
