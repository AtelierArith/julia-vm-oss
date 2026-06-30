using Test

function make_memory_vector_6652(::Type{T}, values) where T
    mem = Memory{T}(undef, length(values))
    for i in 1:length(values)
        mem[i] = values[i]
    end
    return Base.wrap(Array, mem, length(values))
end

function make_offset_vector_6652(::Type{T}, values, start, len) where T
    mem = Memory{T}(undef, length(values))
    for i in 1:length(values)
        mem[i] = values[i]
    end
    return Base.wrap(Array, memoryref(mem, start), len)
end

function make_memory_matrix_6652(::Type{T}, values, dims) where T
    mem = Memory{T}(undef, length(values))
    for i in 1:length(values)
        mem[i] = values[i]
    end
    return Base.wrap(Array, mem, dims)
end

function is_memoryref_array_6652(a, ::Type{T}) where T
    return isa(a, Array) && typeof(a.ref) == MemoryRef{T}
end

@testset "Array wrapper HOF and broadcast over MemoryRef (#6652)" begin
    a = make_memory_vector_6652(Int64, [1, 2, 3, 4, 5])
    offset = make_offset_vector_6652(Int64, [10, 20, 30, 40, 50, 60], 3, 3)

    copied = collect(a)
    @test copied == [1, 2, 3, 4, 5]
    @test is_memoryref_array_6652(copied, Int64)

    offset_copy = collect(offset)
    @test offset_copy == [30, 40, 50]
    @test is_memoryref_array_6652(offset_copy, Int64)

    mapped = map(x -> x + 1, a)
    @test mapped == [2, 3, 4, 5, 6]
    @test is_memoryref_array_6652(mapped, Int64)

    mapped_binary = map(+, offset, make_memory_vector_6652(Int64, [1, 2, 3]))
    @test mapped_binary == [31, 42, 53]
    @test is_memoryref_array_6652(mapped_binary, Int64)

    map_dest = similar(a)
    @test map!(x -> x * 2, map_dest, a) === map_dest
    @test map_dest == [2, 4, 6, 8, 10]
    @test is_memoryref_array_6652(map_dest, Int64)

    map_binary_dest = similar(offset)
    @test map!(+, map_binary_dest, offset, make_memory_vector_6652(Int64, [1, 1, 1])) === map_binary_dest
    @test map_binary_dest == [31, 41, 51]
    @test is_memoryref_array_6652(map_binary_dest, Int64)

    filtered = filter(isodd, a)
    @test filtered == [1, 3, 5]
    @test is_memoryref_array_6652(filtered, Int64)

    filter_dest = collect(a)
    @test filter!(x -> x > 2, filter_dest) === filter_dest
    @test filter_dest == [3, 4, 5]
    @test is_memoryref_array_6652(filter_dest, Int64)

    @test reduce(+, a) == 15
    @test reduce(*, make_memory_vector_6652(Int64, [2, 3, 4])) == 24
    @test mapreduce(x -> x * 2, +, a) == 30

    generator_collect = collect(x * 2 for x in a)
    @test generator_collect == [2, 4, 6, 8, 10]
    @test is_memoryref_array_6652(generator_collect, Int64)

    comp = [x * 3 for x in a]
    @test comp == [3, 6, 9, 12, 15]
    @test is_memoryref_array_6652(comp, Int64)

    filtered_comp = [x * 3 for x in a if isodd(x)]
    @test filtered_comp == [3, 9, 15]
    @test is_memoryref_array_6652(filtered_comp, Int64)

    sorted = sort(make_memory_vector_6652(Int64, [4, 2, 5, 1, 3]))
    @test sorted == [1, 2, 3, 4, 5]
    @test is_memoryref_array_6652(sorted, Int64)

    sorted_rev = sort(make_memory_vector_6652(Int64, [4, 2, 5, 1, 3]); rev=true)
    @test sorted_rev == [5, 4, 3, 2, 1]
    @test is_memoryref_array_6652(sorted_rev, Int64)

    broadcasted = broadcast(x -> x + 10, a)
    @test broadcasted == [11, 12, 13, 14, 15]
    @test is_memoryref_array_6652(broadcasted, Int64)

    dotted = a .+ 2
    @test dotted == [3, 4, 5, 6, 7]
    @test is_memoryref_array_6652(dotted, Int64)

    broadcast_binary = broadcast(+, a, a)
    @test broadcast_binary == [2, 4, 6, 8, 10]
    @test is_memoryref_array_6652(broadcast_binary, Int64)

    broadcast_dest = similar(a)
    @test broadcast!(x -> x + 3, broadcast_dest, a) === broadcast_dest
    @test broadcast_dest == [4, 5, 6, 7, 8]
    @test is_memoryref_array_6652(broadcast_dest, Int64)

    mat = make_memory_matrix_6652(Int64, [1, 2, 3, 4, 5, 6], (2, 3))
    mat_inc = broadcast(x -> x + 1, mat)
    @test size(mat_inc) == (2, 3)
    @test mat_inc == [2 4 6; 3 5 7]
    @test is_memoryref_array_6652(mat_inc, Int64)

    mat_scalar = broadcast(+, mat, 10)
    @test mat_scalar == [11 13 15; 12 14 16]
    @test is_memoryref_array_6652(mat_scalar, Int64)
end

true
