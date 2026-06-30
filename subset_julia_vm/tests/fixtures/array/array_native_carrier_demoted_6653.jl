using Test

function is_memoryref_array_6653(a, ::Type{T}, dims) where T
    return isa(a, Array) && typeof(a.ref) == MemoryRef{T} && a.size == dims
end

function fill_linear_6653!(a)
    for i in 1:length(a)
        a[i] = i
    end
    return a
end

@testset "Public Array routes use MemoryRef-backed wrappers (#6653)" begin
    lit = [1, 2, 3]
    @test lit == [1, 2, 3]
    @test is_memoryref_array_6653(lit, Int64, (3,))

    typed_lit = Int64[4, 5, 6]
    @test typed_lit == [4, 5, 6]
    @test is_memoryref_array_6653(typed_lit, Int64, (3,))

    empty = Vector{Int64}()
    @test empty == Int64[]
    @test is_memoryref_array_6653(empty, Int64, (0,))

    undef_vec = fill_linear_6653!(Array{Int64}(undef, 3))
    @test undef_vec == [1, 2, 3]
    @test is_memoryref_array_6653(undef_vec, Int64, (3,))

    undef_mat = fill_linear_6653!(Array{Int64}(undef, (2, 2)))
    @test undef_mat == [1 3; 2 4]
    @test is_memoryref_array_6653(undef_mat, Int64, (2, 2))

    range_collect = collect(1:3)
    @test range_collect == [1, 2, 3]
    @test is_memoryref_array_6653(range_collect, Int64, (3,))

    tuple_collect = collect((1, 2, 3))
    @test tuple_collect == [1, 2, 3]
    @test is_memoryref_array_6653(tuple_collect, Int64, (3,))

    generator_collect = collect(x * 2 for x in lit)
    @test generator_collect == [2, 4, 6]
    @test is_memoryref_array_6653(generator_collect, Int64, (3,))

    comp = [x + 1 for x in lit]
    @test comp == [2, 3, 4]
    @test is_memoryref_array_6653(comp, Int64, (3,))

    mapped = map(x -> x + 1, lit)
    @test mapped == [2, 3, 4]
    @test is_memoryref_array_6653(mapped, Int64, (3,))

    filtered = filter(isodd, lit)
    @test filtered == [1, 3]
    @test is_memoryref_array_6653(filtered, Int64, (2,))

    sorted = sort([3, 1, 2])
    @test sorted == [1, 2, 3]
    @test is_memoryref_array_6653(sorted, Int64, (3,))

    broadcasted = broadcast(+, lit, lit)
    @test broadcasted == [2, 4, 6]
    @test is_memoryref_array_6653(broadcasted, Int64, (3,))

    similar_vec = similar(lit)
    @test is_memoryref_array_6653(similar_vec, Int64, (3,))

    zeros_vec = zeros(Int64, 3)
    @test zeros_vec == [0, 0, 0]
    @test is_memoryref_array_6653(zeros_vec, Int64, (3,))

    ones_vec = ones(Int64, 3)
    @test ones_vec == [1, 1, 1]
    @test is_memoryref_array_6653(ones_vec, Int64, (3,))

    reshaped = reshape([1, 2, 3, 4], (2, 2))
    @test reshaped == [1 3; 2 4]
    @test is_memoryref_array_6653(reshaped, Int64, (2, 2))
end

true
