# Issue #6640: assigning to an indexed struct field — `obj.field[i] = v` and the
# compound form `obj.field[i] += v` — now lowers to `setindex!(getfield(obj,
# :field), ...)` instead of erroring with UnsupportedAssignmentTarget. Previously
# every Pure Julia collection struct had to bind the field to a local first
# (`d = obj.field; d[i] = v`). Verified against upstream Julia 1.12.

using Test

mutable struct Box
    data::Vector{Int}
end

mutable struct Mem{T}
    mem::Memory{T}
end

function plain_ok()
    b = Box([10, 20, 30])
    b.data[1] = 99
    b.data[3] = -1
    return b.data[1] == 99 && b.data[2] == 20 && b.data[3] == -1
end

function compound_ok()
    b = Box([10, 20])
    b.data[1] += 5
    b.data[2] *= 2
    return b.data[1] == 15 && b.data[2] == 40
end

function memory_field_ok()
    m = Mem{Int}(Memory{Int}(undef, 3))
    m.mem[1] = 7
    m.mem[2] = 8
    m.mem[1] += 10
    return m.mem[1] == 17 && m.mem[2] == 8
end

all_ok() = plain_ok() && compound_ok() && memory_field_ok()

@testset "indexed assignment to a struct field (#6640)" begin
    @test plain_ok()
    @test compound_ok()
    @test memory_field_ok()
end

all_ok()
