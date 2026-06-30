using Test

@testset "IndexStore routes Tuple/Array/boxed targets through helper (Issue #3908)" begin
    # Tuple-value IndexStore branch: store a Tuple into a Vector{Tuple{Int,Int}}.
    tuple_storage = Vector{Tuple{Int,Int}}(undef, 2)
    tuple_storage[1] = (1, 2)
    tuple_storage[2] = (3, 4)

    @test tuple_storage[1] == (1, 2)
    @test tuple_storage[2] == (3, 4)
    @test length(tuple_storage) == 2

    # Array-element IndexStore branch (Issue #3648): store a Vector{Int} into a
    # heterogeneous Vector{Any}.
    nested = Vector{Any}(undef, 2)
    nested[1] = [10, 20]
    nested[2] = [30, 40, 50]

    @test nested[1] == [10, 20]
    @test nested[2] == [30, 40, 50]
    @test length(nested) == 2
    @test length(nested[2]) == 3

    # Boxed-scalar IndexStore branch (String/Char/Symbol path).
    str_storage = Vector{String}(undef, 2)
    str_storage[1] = "alpha"
    str_storage[2] = "beta"

    @test str_storage[1] == "alpha"
    @test str_storage[2] == "beta"

    char_storage = Vector{Char}(undef, 3)
    char_storage[1] = 'a'
    char_storage[2] = 'b'
    char_storage[3] = 'c'

    @test char_storage[1] == 'a'
    @test char_storage[2] == 'b'
    @test char_storage[3] == 'c'

    sym_storage = Vector{Symbol}(undef, 2)
    sym_storage[1] = :first
    sym_storage[2] = :second

    @test sym_storage[1] === :first
    @test sym_storage[2] === :second
end

true
