# Test that length/size/ndims dispatch through Pure Julia method tables
# (Issue #3736). When a user-defined struct provides shape methods, normal
# dispatch must select them instead of routing through the Rust BuiltinId
# handlers (BuiltinId::Length / BuiltinId::Size / BuiltinId::Ndims).

using Test

struct ShapeBox
    n::Int64
end

# User-provided dispatch targets: these must win over the builtin path.
function length(b::ShapeBox)
    return b.n + 1000
end

function size(b::ShapeBox)
    return (b.n, b.n + 1)
end

function size(b::ShapeBox, dim::Int64)
    return b.n * dim
end

function ndims(b::ShapeBox)
    return b.n + 7
end

@testset "length/size/ndims dispatch on user-defined struct (Issue #3736)" begin
    b = ShapeBox(3)
    @test (length(b) == 1003)
    @test (size(b) == (3, 4))
    @test (size(b, 2) == 6)
    @test (ndims(b) == 10)
end

true
