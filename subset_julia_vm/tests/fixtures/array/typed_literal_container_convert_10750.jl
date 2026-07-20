using Test

function typed_literal_container_convert_10750(x)
    boxed = Vector{Int}[x]
    return boxed[1]
end

@testset "typed array literal converts parametric container elements (Issue #10750)" begin
    converted = typed_literal_container_convert_10750([1.0, 2.0, 3.0])
    @test converted == [1, 2, 3]
    @test typeof(converted) === Vector{Int64}
    @test eltype(converted) === Int64
end

@testset "typed array literals always use convert (Issues #10835, #11779)" begin
    converted = Char[97]
    @test converted == ['a']
    @test typeof(converted[1]) === Char

    # Exact-type boxed values still take convert(::Type{T}, x::T) = x. Object
    # identity across Regex array storage is tracked separately in Issue #11780.
    regex = r"typed"
    boxed = Regex[regex]
    @test boxed[1] == regex

    nested_union = Union{Nothing,SubString{String}}["x", nothing]
    @test nested_union == ["x", nothing]
    @test eltype(nested_union) === Union{Nothing,SubString{String}}

    # The target expression is evaluated once before the elements. The first
    # element may mutate a binding used to construct that target, but the
    # second element must still convert to the original target.
    global typed_literal_target_10835 = Int64
    mutate_target_10835() = (global typed_literal_target_10835 = Float64; [1])
    snapshotted = Vector{typed_literal_target_10835}[mutate_target_10835(), [2]]
    @test snapshotted == [[1], [2]]
    # The separate logical-eltype metadata gap is tracked by Issue #11787.
    @test typeof(snapshotted[1]) === Vector{Int64}
    @test typeof(snapshotted[2]) === Vector{Int64}
    @test typed_literal_target_10835 === Float64
end

true
