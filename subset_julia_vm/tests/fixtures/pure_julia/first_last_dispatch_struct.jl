# Test that first/last dispatch through Pure Julia method tables (Issue #3734).
# When a user-defined struct provides a `first` or `last` method, normal dispatch
# must select it instead of routing through the Rust TupleFirst/TupleLast builtins.

using Test

struct Box
    value::Int64
end

# User-provided dispatch targets: these must win over any builtin routing.
function first(b::Box)
    return b.value + 100
end

function last(b::Box)
    return b.value + 200
end

@testset "first/last dispatch on user-defined struct (Issue #3734)" begin
    b = Box(7)
    @test (first(b) == 107)
    @test (last(b) == 207)
end

true
