# Issue #4774: matrix fixture covering Container<user_struct> /
# Container<Pair> for every display entry point. Prevention sibling
# of #4757 / #4766 / #4772 — those covered (entry point) × (top-level
# value class) but did NOT cover (container shape × heap-allocated
# element). The PR #4771 / #4773 round-trip showed this gap is real.
#
# This fixture exercises 7+1 container shapes × 6 entry points and
# asserts no `StructRef(` / `heap_idx=` / `I64(` / `Str(` Rust Debug
# tokens leak in any cell.

using Test

struct ContainerInner4774
    x::Int64
    y::Int64
end

function no_debug_leak_4774(s)
    bad_tokens = ("StructRef(", "heap_idx=",
                  "I64(", "I32(", "I128(", "U64(", "F64(",
                  "Str(", "Char(", "Symbol(")
    for tok in bad_tokens
        if occursin(tok, s)
            return false
        end
    end
    return true
end

function via_print_buf(c)
    b = IOBuffer()
    print(b, c)
    return String(take!(b))
end

function via_print_buf_multiarg(c)
    b = IOBuffer()
    print(b, "[", c, "]")
    return String(take!(b))
end

function via_sprint(c)
    return sprint(io -> print(io, c))
end

function entry_points(c)
    return (
        string(c),
        repr(c),
        via_print_buf(c),
        via_print_buf_multiarg(c),
        via_sprint(c),
        "interp: $c"
    )
end

@testset "no leak — Vector<user_struct> (Issue #4774)" begin
    inner = ContainerInner4774(7, 9)
    c = [inner, inner]
    for s in entry_points(c)
        @test no_debug_leak_4774(s)
    end
end

@testset "no leak — Vector<Pair> (Issue #4774)" begin
    c = [Pair(1, 2), Pair(3, 4)]
    for s in entry_points(c)
        @test no_debug_leak_4774(s)
    end
end

@testset "no leak — Tuple<user_struct> (Issue #4774)" begin
    inner = ContainerInner4774(1, 2)
    c = (inner, inner)
    for s in entry_points(c)
        @test no_debug_leak_4774(s)
    end
end

@testset "no leak — NamedTuple<user_struct> (Issue #4774)" begin
    inner = ContainerInner4774(3, 4)
    c = (a = inner, b = inner)
    for s in entry_points(c)
        @test no_debug_leak_4774(s)
    end
end

@testset "no leak — Dict<value=user_struct> (Issue #4774)" begin
    inner = ContainerInner4774(5, 6)
    c = Dict("k" => inner)
    for s in entry_points(c)
        @test no_debug_leak_4774(s)
    end
end

@testset "no leak — Pair<user_struct, Int> (Issue #4774)" begin
    inner = ContainerInner4774(7, 8)
    c = Pair(inner, 1)
    for s in entry_points(c)
        @test no_debug_leak_4774(s)
    end
end

@testset "no leak — Ref<user_struct> (Issue #4774)" begin
    inner = ContainerInner4774(9, 10)
    c = Ref(inner)
    for s in entry_points(c)
        @test no_debug_leak_4774(s)
    end
end

@testset "no leak — Set<primitive> (Issue #4774)" begin
    # Set elements are DictKeys, not Values, so this is a different
    # leak class — previously Set([1,2,3]) leaked `I64(1)` via the
    # format_value Set arm's Rust Debug fallback.
    for s in entry_points(Set([1, 2, 3]))
        @test no_debug_leak_4774(s)
    end
    for s in entry_points(Set(["a", "b"]))
        @test no_debug_leak_4774(s)
    end
end

true
