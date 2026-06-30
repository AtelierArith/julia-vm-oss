# Test that getindex / setindex! dispatch through Pure Julia method tables
# (Issue #3729). When a user-defined struct provides indexing methods,
# normal dispatch must select them instead of routing through the Rust
# `compile_builtin_call` path used for primitive Array/Dict/String containers.

using Test

mutable struct Slot
    v::Int64
end

# Pure Julia getindex / setindex! on a mutable user struct.
# Extending Base.getindex / Base.setindex! so syntax sugar (s[i], s[i] = v)
# resolves through the same method table.
function Base.getindex(s::Slot, i::Int64)
    return s.v + i * 1000
end

function Base.setindex!(s::Slot, val::Int64, i::Int64)
    s.v = val + i * 10
    return s
end

@testset "user-defined getindex Pure Julia dispatch (Issue #3729)" begin
    s = Slot(7)
    @test (getindex(s, 1) == 1007)
    @test (getindex(s, 3) == 3007)
end

@testset "user-defined setindex! Pure Julia dispatch (Issue #3729)" begin
    s = Slot(7)
    setindex!(s, 100, 5)
    @test (s.v == 150)
end

true
