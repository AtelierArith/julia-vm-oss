# Issue #4766 (prevention for fix family #4761 / #4763, sibling of
# #4757's matrix): extend the "no Rust Debug leak" matrix to the
# IOBuffer / sprint / multi-arg print entry points. These paths
# previously leaked `StructRef(heap_idx=N)` because they fed raw
# popped Values to `format_value_print` without first resolving
# `Value::StructRef` against the struct heap (only the single-arg
# `print(x)` arm did that resolution).
#
# Matrix dimensions:
#   entry points  : print(buf, x), write(buf, x), sprint(io -> print(io, x)),
#                   multi-arg print(buf, "[", x, "]"), interpolation, string, repr
#   value classes : Pair (heap-allocated struct), user struct, nested
#                   Pair-inside-Tuple, Int128, Symbol, Char, String
#
# Runtime-agnostic: the bad tokens don't occur in upstream Julia's
# own display form either, so the fixture passes under both sjulia
# and julia.

using Test

function no_debug_leak_4766(s)
    bad_tokens = ("StructRef(", "heap_idx=",
                  "I8(", "I16(", "I32(", "I128(",
                  "U8(", "U16(", "U32(", "U64(", "U128(",
                  "F16(", "F32(",
                  "Symbol(")
    for tok in bad_tokens
        if occursin(tok, s)
            return false
        end
    end
    return true
end

function via_print_buf(x)
    buf = IOBuffer()
    print(buf, x)
    return String(take!(buf))
end

function via_write_buf(x)
    buf = IOBuffer()
    write(buf, x)
    return String(take!(buf))
end

function via_sprint_print(x)
    return sprint(io -> print(io, x))
end

function via_print_buf_multiarg(x)
    buf = IOBuffer()
    print(buf, "[", x, "]")
    return String(take!(buf))
end

struct MatrixFoo4766
    x::Int64
    y::Int64
end

@testset "no Rust Debug leak — print(buf, x) (Issue #4766)" begin
    @test no_debug_leak_4766(via_print_buf(Pair(1, 2)))
    @test no_debug_leak_4766(via_print_buf(MatrixFoo4766(3, 4)))
    @test no_debug_leak_4766(via_print_buf(Int128(1) << 60))
    @test no_debug_leak_4766(via_print_buf(:foo))
    @test no_debug_leak_4766(via_print_buf('a'))
end

@testset "no Rust Debug leak — write(buf, x) for non-error values (Issue #4766)" begin
    # `write(buf, ::Pair)` errors in upstream Julia (no method), so
    # restrict this row to value classes that `write` natively accepts.
    @test no_debug_leak_4766(via_write_buf("hello"))
    @test no_debug_leak_4766(via_write_buf(Int8(42)))
    @test no_debug_leak_4766(via_write_buf(UInt64(7)))
end

@testset "no Rust Debug leak — sprint(io -> print(io, x)) (Issue #4766)" begin
    @test no_debug_leak_4766(via_sprint_print(Pair(1, 2)))
    @test no_debug_leak_4766(via_sprint_print(MatrixFoo4766(5, 6)))
    @test no_debug_leak_4766(via_sprint_print(Int128(1) << 60))
end

@testset "no Rust Debug leak — print(buf, lit, x, lit) multi-arg (Issue #4766)" begin
    @test no_debug_leak_4766(via_print_buf_multiarg(Pair(7, 8)))
    @test no_debug_leak_4766(via_print_buf_multiarg(MatrixFoo4766(9, 10)))
end

true
