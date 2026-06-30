using Test

function make_memory_vector_6651(values)
    mem = Memory{Int64}(undef, length(values))
    for i in 1:length(values)
        mem[i] = values[i]
    end
    return Base.wrap(Array, mem, length(values)), mem
end

function make_offset_vector_6651()
    mem = Memory{Int64}(undef, 6)
    for i in 1:6
        mem[i] = 10 * i
    end
    return Base.wrap(Array, memoryref(mem, 3), 3), mem
end

@testset "Array wrapper mutation and iteration over MemoryRef (#6651)" begin
    a, _ = make_memory_vector_6651([1, 2, 3])
    first_next = iterate(a)
    @test first_next == (1, 2)
    second_next = iterate(a, first_next[2])
    @test second_next == (2, 3)
    third_next = iterate(a, second_next[2])
    @test third_next == (3, 4)
    @test iterate(a, third_next[2]) === nothing

    total = 0
    for x in a
        total += x
    end
    @test total == 6

    @test push!(a, 4) === a
    @test collect(a) == [1, 2, 3, 4]
    @test pop!(a) == 4
    @test collect(a) == [1, 2, 3]
    @test pushfirst!(a, 0) === a
    @test collect(a) == [0, 1, 2, 3]
    @test popfirst!(a) == 0
    @test collect(a) == [1, 2, 3]
    @test insert!(a, 2, 9) === a
    @test collect(a) == [1, 9, 2, 3]
    @test deleteat!(a, 2) === a
    @test collect(a) == [1, 2, 3]
    @test append!(a, (4, 5)) === a
    @test collect(a) == [1, 2, 3, 4, 5]
    @test resize!(a, 3) === a
    @test collect(a) == [1, 2, 3]
    @test empty!(a) === a
    @test length(a) == 0

    pushed, pushed_mem = make_offset_vector_6651()
    @test collect(pushed) == [30, 40, 50]
    @test push!(pushed, 99) === pushed
    @test collect(pushed) == [30, 40, 50, 99]
    pushed[1] = 77
    @test pushed_mem[3] == 77

    popped, popped_mem = make_offset_vector_6651()
    @test pop!(popped) == 50
    @test collect(popped) == [30, 40]
    popped[1] = 71
    @test popped_mem[3] == 71

    shifted_first, shifted_first_mem = make_offset_vector_6651()
    @test popfirst!(shifted_first) == 30
    @test collect(shifted_first) == [40, 50]
    shifted_first[1] = 72
    @test shifted_first_mem[4] == 72

    prepended, prepended_mem = make_offset_vector_6651()
    @test pushfirst!(prepended, 22) === prepended
    @test collect(prepended) == [22, 30, 40, 50]
    prepended[1] = 73
    @test prepended_mem[2] == 73
    @test prepended_mem[3] == 30

    inserted, inserted_mem = make_offset_vector_6651()
    @test insert!(inserted, 2, 88) === inserted
    @test collect(inserted) == [30, 88, 40, 50]
    @test inserted_mem[3] == 30
    @test inserted_mem[4] == 88
    @test inserted_mem[5] == 40
    @test inserted_mem[6] == 50

    deleted_middle, deleted_middle_mem = make_offset_vector_6651()
    @test deleteat!(deleted_middle, 2) === deleted_middle
    @test collect(deleted_middle) == [30, 50]
    deleted_middle[1] = 74
    @test deleted_middle_mem[3] == 74
    @test deleted_middle_mem[4] == 50

    deleted_first, deleted_first_mem = make_offset_vector_6651()
    @test deleteat!(deleted_first, 1) === deleted_first
    @test collect(deleted_first) == [40, 50]
    deleted_first[1] = 75
    @test deleted_first_mem[3] == 30
    @test deleted_first_mem[4] == 75

    resized, resized_mem = make_offset_vector_6651()
    @test resize!(resized, 4) === resized
    resized[4] = 76
    @test resized_mem[6] == 76
    @test resize!(resized, 2) === resized
    resized[1] = 77
    @test resized_mem[3] == 77
end

true
