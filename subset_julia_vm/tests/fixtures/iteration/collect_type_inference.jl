# Test collect type inference
# collect should return Int64 array for integer ranges

using Test

range_eltype_runtime(x) = eltype(x)
range_collect_trait_runtime(x) = Base._collect(1:1, x, Base.HasEltype(), Base.HasLength())
range_collect_runtime(x) = collect(x)

@testset "collect returns typed arrays (Int64 for integer ranges)" begin

    # Integer range -> Int64 array
    x = collect(1:5)
    @test typeof(x) === Vector{Int64}
    @assert x == [1, 2, 3, 4, 5]
    @assert size(x, 1) == 5
    x[3] = 30
    @assert x == [1, 2, 30, 4, 5]

    # Float range -> Float64 array
    y = collect(1.0:0.5:3.0)
    @test typeof(y) === Vector{Float64}
    @assert length(y) == 5
    @assert size(y, 1) == 5

    # Step range with integers -> Int64 array
    z = collect(1:2:9)
    @test typeof(z) === Vector{Int64}
    @assert z == [1, 3, 5, 7, 9]

    # Negative step range
    w = collect(5:-1:1)
    @test typeof(w) === Vector{Int64}
    @assert w == [5, 4, 3, 2, 1]

    # Empty range keeps typed vector shape
    empty = collect(5:4)
    @test typeof(empty) === Vector{Int64}
    @assert size(empty, 1) == 0

    # Tuple collection still materializes as a fresh vector
    tuple_values = collect((1, 2, 3))
    @test typeof(tuple_values) === Vector{Int64}
    @assert tuple_values == [1, 2, 3]

    @testset "Range collect trait path (Issues #4065/#4066)" begin
        @test typeof(Base.IteratorEltype(1:5)) === typeof(Base.HasEltype())
        @test typeof(Base.IteratorSize(1:5)) !== typeof(Base.SizeUnknown())

        @test range_eltype_runtime(1:5) === Int64
        @test range_eltype_runtime(5:4) === Int64
        @test range_eltype_runtime(1:2:9) === Int64
        @test range_eltype_runtime(5:-1:1) === Int64
        @test range_eltype_runtime(1.0:0.5:3.0) === Float64
        @test range_eltype_runtime("abc") === Char

        trait_values = range_collect_trait_runtime(1:5)
        @test typeof(trait_values) === Vector{Int64}
        @test trait_values == [1, 2, 3, 4, 5]

        empty_values = range_collect_trait_runtime(5:4)
        @test typeof(empty_values) === Vector{Int64}
        @test length(empty_values) == 0

        step_values = range_collect_trait_runtime(1:2:9)
        @test typeof(step_values) === Vector{Int64}
        @test step_values == [1, 3, 5, 7, 9]

        reverse_values = range_collect_trait_runtime(5:-1:1)
        @test typeof(reverse_values) === Vector{Int64}
        @test reverse_values == [5, 4, 3, 2, 1]

        float_values = range_collect_trait_runtime(1.0:0.5:3.0)
        @test typeof(float_values) === Vector{Float64}
        @test float_values == [1.0, 1.5, 2.0, 2.5, 3.0]
    end

    @testset "Runtime Range collect preserves eltype (Issue #4075)" begin
        runtime_values = range_collect_runtime(1:5)
        @test typeof(runtime_values) === Vector{Int64}
        @test runtime_values == [1, 2, 3, 4, 5]

        runtime_empty = range_collect_runtime(5:4)
        @test typeof(runtime_empty) === Vector{Int64}
        @test length(runtime_empty) == 0

        runtime_step = range_collect_runtime(1:2:9)
        @test typeof(runtime_step) === Vector{Int64}
        @test runtime_step == [1, 3, 5, 7, 9]

        runtime_reverse = range_collect_runtime(5:-1:1)
        @test typeof(runtime_reverse) === Vector{Int64}
        @test runtime_reverse == [5, 4, 3, 2, 1]

        runtime_float = range_collect_runtime(1.0:0.5:3.0)
        @test typeof(runtime_float) === Vector{Float64}
        @test runtime_float == [1.0, 1.5, 2.0, 2.5, 3.0]
    end

    @testset "Struct-backed Range collect preserves eltype (Issue #4078)" begin
        lin = collect(range(1.0, 3.0, length=3))
        @test typeof(lin) === Vector{Float64}
        @test lin == [1.0, 2.0, 3.0]

        step_len = collect(range(1.0, step=0.5, length=3))
        @test typeof(step_len) === Vector{Float64}
        @test step_len == [1.0, 1.5, 2.0]
    end

    @testset "HasLength collect allocation path (Issue #4081)" begin
        known_length = Base._collect(1:1, 1:5, Base.HasEltype(), Base.HasLength())
        @test typeof(known_length) === Vector{Int64}
        @test length(known_length) == 5
        @test known_length == [1, 2, 3, 4, 5]
        known_length[3] = 30
        @test known_length == [1, 2, 30, 4, 5]

        known_length_float = Base._collect(1:1, 1.0:0.5:2.0, Base.HasEltype(), Base.HasLength())
        @test typeof(known_length_float) === Vector{Float64}
        @test known_length_float == [1.0, 1.5, 2.0]
    end

    @testset "Array HasShape collect path (Issue #4083)" begin
        vector = [1, 2, 3]
        @test typeof(Base.IteratorSize(vector)) === Base.HasShape{1}

        matrix = [1 2; 3 4]
        matrix_size = Base.IteratorSize(matrix)
        @test typeof(matrix_size) === Base.HasShape{2}

        matrix_values = Base._collect(1:1, matrix, Base.IteratorEltype(matrix), matrix_size)
        @test typeof(matrix_values) === Matrix{Int64}
        @test size(matrix_values) == (2, 2)
        @test matrix_values == matrix

        float_matrix = [1.0 2.0; 3.0 4.0]
        float_values = Base._collect(1:1, float_matrix, Base.IteratorEltype(float_matrix), Base.IteratorSize(float_matrix))
        @test typeof(float_values) === Matrix{Float64}
        @test size(float_values) == (2, 2)
        @test float_values == float_matrix
    end

    @testset "EltypeUnknown HasLength collect widening (Issue #4087)" begin
        direct_tuple = Base._collect(1:1, (1, 2.0), Base.EltypeUnknown(), Base.HasLength())
        @test typeof(direct_tuple) === Vector{Real}
        @test length(direct_tuple) == 2
        @test direct_tuple[1] == 1
        @test direct_tuple[2] == 2.0
    end

    @testset "Array collect helper slice (Issue #4052)" begin
        helper_len = Base._similar_shape(1:3, Base.HasLength())
        @test helper_len == 3

        helper_vector = Base._array_for_inner(Float64, Base.HasLength(), helper_len)
        @test typeof(helper_vector) === Vector{Float64}
        @test length(helper_vector) == 3

        helper_matrix_src = [1 2; 3 4]
        helper_shape = Base._similar_shape(helper_matrix_src, Base.IteratorSize(helper_matrix_src))
        helper_matrix = Base._array_for_inner(Float64, Base.IteratorSize(helper_matrix_src), helper_shape)
        @test typeof(helper_matrix) === Matrix{Float64}
        @test size(helper_matrix) == (2, 2)

        seeded_itr = (10, 20, 30)
        seeded_first, seeded_state = iterate(seeded_itr)
        seeded = Vector{Int64}(undef, 3)
        filled = Base.collect_to_with_first!(seeded, seeded_first, seeded_itr, seeded_state)
        @test filled === seeded
        @test filled == [10, 20, 30]

        widened_itr = (1, 2.0)
        widened_first, widened_state = iterate(widened_itr)
        widened = Base.collect_to_with_first!(Vector{Int64}(undef, 2), widened_first, widened_itr, widened_state)
        @test typeof(widened) === Vector{Real}
        @test widened == [1, 2.0]
    end

    # Return true to indicate success
    @test (true)
end

true  # Test passed
