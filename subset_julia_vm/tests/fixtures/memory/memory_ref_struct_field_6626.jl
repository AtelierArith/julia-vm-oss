# Issue #6626: a `MemoryRef{T}` value (from `memoryref(mem)`) can be stored in a
# parametric `ref::MemoryRef{T}` struct field and recovered with the right type
# — the upstream `Array{T,N}` shape (`ref::MemoryRef{T}` + size). Methods can
# also dispatch on a `::MemoryRef` parameter. Verified against upstream Julia.
#
# Builds on #6623 (Memory{K}/MemoryRef{K} field-type resolution) plus an
# `Any -> Memory` coercion so the `memoryref(...)` value (inferred `Any`) flows
# into the typed field.

using Test

# The upstream Array{T} wrapper shape: a MemoryRef plus a length.
mutable struct ArrLike{T}
    ref::MemoryRef{T}
    len::Int
end

# Dispatch on a ::MemoryRef parameter (as base/genericmemory.jl does). Returns a
# marker so the test is portable (no unexported Core intrinsic).
ref_kind(r::MemoryRef) = "memref"
ref_kind(x) = "other"

function field_ok()
    m = Memory{Int}(undef, 4)
    _m = m
    _m[1] = 7
    r = memoryref(m)
    a = ArrLike{Int}(r, 4)
    a1 = typeof(a) == ArrLike{Int}
    _r = a.ref
    a2 = typeof(_r) == MemoryRef{Int}
    a3 = a.len == 4
    return a1 && a2 && a3
end

function string_elt_ok()
    m = Memory{String}(undef, 2)
    _m = m
    _m[1] = "x"
    a = ArrLike{String}(memoryref(m), 2)
    return typeof(a) == ArrLike{String} && a.len == 2
end

function dispatch_ok()
    m = Memory{Int}(undef, 2)
    r = memoryref(m)
    # the ::MemoryRef method must win over the ::Any fallback
    return ref_kind(r) == "memref" && ref_kind(5) == "other"
end

# Memory / MemoryRef are first-class type values: isa, parametric isa, <:.
function type_value_ok()
    m = Memory{Int}(undef, 2)
    r = memoryref(m)
    return (m isa Memory) && (m isa Memory{Int}) &&
           (r isa MemoryRef) && (r isa MemoryRef{Int}) &&
           (Memory{Int} <: Memory) && (MemoryRef{Int} <: MemoryRef)
end

all_ok() = field_ok() && string_elt_ok() && dispatch_ok() && type_value_ok()

@testset "MemoryRef{T} struct field + dispatch (#6626)" begin
    @test field_ok()
    @test string_elt_ok()
    @test dispatch_ok()
    @test type_value_ok()
end

all_ok()
