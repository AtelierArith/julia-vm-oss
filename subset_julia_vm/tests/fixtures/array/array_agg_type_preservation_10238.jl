# Aggregated fixtures with top-level definitions, isolated by module wrapping
# (Issue #10238; Issue #9671 Phase 3 continuation, unblocked by the #9942 fix).
# Each block below is one former standalone fixture, verbatim except its
# trailing protocol `true`, wrapped in its own `module Agg_<stem>` so top-level
# struct/function/const/global definitions stay namespaced and cannot collide.
# `using Test` stays inside each module (modules do not inherit imports).
# @testset names (with their original Issue numbers) are preserved, and the
# #9360 @testset gate still detects any per-@testset failure.
# Source fixture in each banner.

# ===== source: array/accumulate_operator_type_preservation_4608.jl =====
module Agg_accumulate_operator_type_preservation_4608
using Test

@testset "accumulate operator paths preserve typed allocation (#4018, #4608)" begin
    i8 = zeros(Int8, 3)
    i8[1] = 1
    i8[2] = 2
    i8[3] = 3
    ri8 = accumulate(+, i8)
    @test typeof(ri8) == Vector{Int8}
    @test eltype(ri8) == Int8
    @test typeof(ri8[1]) == Int8
    @test ri8 == Int8[1, 3, 6]

    i16 = zeros(Int16, 3)
    i16[1] = 1
    i16[2] = 2
    i16[3] = 3
    ri16 = accumulate(*, i16)
    @test typeof(ri16) == Vector{Int16}
    @test eltype(ri16) == Int16
    @test typeof(ri16[1]) == Int16
    @test ri16 == Int16[1, 2, 6]

    i32 = zeros(Int32, 3)
    i32[1] = 1
    i32[2] = 2
    i32[3] = 3
    ri32 = accumulate(+, i32)
    @test typeof(ri32) == Vector{Int32}
    @test eltype(ri32) == Int32
    @test typeof(ri32[1]) == Int32
    @test ri32 == Int32[1, 3, 6]

    u8 = zeros(UInt8, 3)
    u8[1] = 1
    u8[2] = 2
    u8[3] = 3
    ru8 = accumulate(+, u8)
    @test typeof(ru8) == Vector{UInt8}
    @test eltype(ru8) == UInt8
    @test typeof(ru8[1]) == UInt8
    @test ru8 == UInt8[1, 3, 6]

    u16 = zeros(UInt16, 3)
    u16[1] = 1
    u16[2] = 2
    u16[3] = 3
    ru16 = accumulate(*, u16)
    @test typeof(ru16) == Vector{UInt16}
    @test eltype(ru16) == UInt16
    @test typeof(ru16[1]) == UInt16
    @test ru16 == UInt16[1, 2, 6]

    u32 = zeros(UInt32, 3)
    u32[1] = 1
    u32[2] = 2
    u32[3] = 3
    ru32 = accumulate(+, u32)
    @test typeof(ru32) == Vector{UInt32}
    @test eltype(ru32) == UInt32
    @test typeof(ru32[1]) == UInt32
    @test ru32 == UInt32[1, 3, 6]

    u64 = zeros(UInt64, 3)
    u64[1] = 1
    u64[2] = 2
    u64[3] = 3
    ru64 = accumulate(*, u64)
    @test typeof(ru64) == Vector{UInt64}
    @test eltype(ru64) == UInt64
    @test typeof(ru64[1]) == UInt64
    @test ru64 == UInt64[1, 2, 6]

    f32 = zeros(Float32, 3)
    f32[1] = 1
    f32[2] = 2
    f32[3] = 3
    rf32 = accumulate(+, f32)
    @test typeof(rf32) == Vector{Float32}
    @test eltype(rf32) == Float32
    @test typeof(rf32[1]) == Float32
    @test rf32 == Float32[1, 3, 6]

    b = Bool[true, false, true]
    rb_sum = accumulate(+, b)
    @test typeof(rb_sum) == Vector{Int64}
    @test eltype(rb_sum) == Int64
    @test typeof(rb_sum[1]) == Int64
    @test rb_sum == [1, 1, 2]

    rb_prod = accumulate(*, b)
    @test typeof(rb_prod) == Vector{Bool}
    @test eltype(rb_prod) == Bool
    @test typeof(rb_prod[1]) == Bool
    @test rb_prod == Bool[true, false, false]

    generic_i8 = accumulate((x, y) -> x + y, i8)
    @test typeof(generic_i8) == Vector{Int8}
    @test eltype(generic_i8) == Int8
    @test typeof(generic_i8[1]) == Int8
    @test generic_i8 == Int8[1, 3, 6]

    generic_i8_to_f32 = accumulate((x, y) -> Float32(x) + Float32(y), i8)
    @test typeof(generic_i8_to_f32) == Vector{Float32}
    @test eltype(generic_i8_to_f32) == Float32
    @test typeof(generic_i8_to_f32[1]) == Float32
    @test generic_i8_to_f32 == Float32[1, 3, 6]

    generic_i8_pairs = accumulate((x, y) -> x => y, i8)
    @test typeof(generic_i8_pairs) == Vector{Any}
    @test eltype(generic_i8_pairs) == Any
    @test generic_i8_pairs[2].first == Int8(1)
    @test generic_i8_pairs[2].second == Int8(2)
    @test generic_i8_pairs[3].second == Int8(3)

    div_one_i8 = accumulate(/, Int8[2])
    @test typeof(div_one_i8) == Vector{Float64}
    @test eltype(div_one_i8) == Float64
    @test typeof(div_one_i8[1]) == Float64
    @test div_one_i8 == Float64[2]

    generic_i8_to_f32_one = accumulate((x, y) -> Float32(x) + Float32(y), Int8[1])
    @test typeof(generic_i8_to_f32_one) == Vector{Float32}
    @test eltype(generic_i8_to_f32_one) == Float32
    @test typeof(generic_i8_to_f32_one[1]) == Float32
    @test generic_i8_to_f32_one == Float32[1]

    generic_i8_pair_one = accumulate((x, y) -> x => y, Int8[1])
    @test typeof(generic_i8_pair_one) == Vector{Any}
    @test eltype(generic_i8_pair_one) == Any
    @test typeof(generic_i8_pair_one[1]) == Int8
    @test generic_i8_pair_one[1] == Int8(1)

    div_empty_i8 = accumulate(/, Int8[])
    @test typeof(div_empty_i8) == Vector{Float64}
    @test eltype(div_empty_i8) == Float64
    @test length(div_empty_i8) == 0

    generic_i8_to_f32_empty = accumulate((x, y) -> Float32(x) + Float32(y), Int8[])
    @test typeof(generic_i8_to_f32_empty) == Vector{Float32}
    @test eltype(generic_i8_to_f32_empty) == Float32
    @test length(generic_i8_to_f32_empty) == 0

    generic_i8_pair_empty = accumulate((x, y) -> x => y, Int8[])
    @test typeof(generic_i8_pair_empty) == Vector{Any}
    @test eltype(generic_i8_pair_empty) == Any
    @test length(generic_i8_pair_empty) == 0

    words = String["a", "b", "c"]
    generic_words = accumulate((x, y) -> x * y, words)
    @test typeof(generic_words) == Vector{String}
    @test eltype(generic_words) == String
    @test typeof(generic_words[1]) == String
    @test generic_words == String["a", "ab", "abc"]

    empty_i8 = accumulate((x, y) -> x + y, Int8[])
    @test typeof(empty_i8) == Vector{Int8}
    @test eltype(empty_i8) == Int8
    @test length(empty_i8) == 0
end
end # module Agg_accumulate_operator_type_preservation_4608

# ===== source: array/adjoint_bool_type_preservation_4601.jl =====
module Agg_adjoint_bool_type_preservation_4601
using Test

@testset "adjoint preserves Bool element type (#4018, #4601)" begin
    v = Bool[true, false]
    row = adjoint(v)
    @test eltype(row) === Bool
    @test size(row) == (1, 2)
    @test typeof(row[1, 1]) === Bool
    @test typeof(row[1, 2]) === Bool
    @test row[1, 1] === true
    @test row[1, 2] === false

    A = reshape(Bool[true, false, true, false], 2, 2)
    transposed = adjoint(A)
    @test eltype(transposed) === Bool
    @test size(transposed) == (2, 2)
    @test typeof(transposed[1, 1]) === Bool
    @test typeof(transposed[1, 2]) === Bool
    @test typeof(transposed[2, 1]) === Bool
    @test typeof(transposed[2, 2]) === Bool
    @test transposed[1, 1] === true
    @test transposed[1, 2] === false
    @test transposed[2, 1] === true
    @test transposed[2, 2] === false
end
end # module Agg_adjoint_bool_type_preservation_4601

# ===== source: array/adjoint_complex_type_preservation_4604.jl =====
module Agg_adjoint_complex_type_preservation_4604
using Test

@testset "adjoint preserves Complex{Float64} element type (#4018, #4604)" begin
    v = zeros(Complex{Float64}, 2)
    v[1] = 1 + 2im
    v[2] = 3 - 4im
    row = adjoint(v)
    @test eltype(row) == Complex{Float64}
    @test size(row) == (1, 2)
    @test typeof(row[1, 1]) == Complex{Float64}
    @test typeof(row[1, 2]) == Complex{Float64}
    @test row[1, 1] == 1 - 2im
    @test row[1, 2] == 3 + 4im

    A = zeros(Complex{Float64}, 2, 2)
    A[1, 1] = 1 + 2im
    A[2, 1] = 3 + 4im
    A[1, 2] = 5 - 6im
    A[2, 2] = 7 - 8im
    transposed = adjoint(A)
    @test eltype(transposed) == Complex{Float64}
    @test size(transposed) == (2, 2)
    @test typeof(transposed[1, 1]) == Complex{Float64}
    @test typeof(transposed[1, 2]) == Complex{Float64}
    @test typeof(transposed[2, 1]) == Complex{Float64}
    @test typeof(transposed[2, 2]) == Complex{Float64}
    @test transposed[1, 1] == 1 - 2im
    @test transposed[1, 2] == 3 - 4im
    @test transposed[2, 1] == 5 + 6im
    @test transposed[2, 2] == 7 + 8im
end
end # module Agg_adjoint_complex_type_preservation_4604

# ===== source: array/adjoint_float32_type_preservation_4597.jl =====
module Agg_adjoint_float32_type_preservation_4597
using Test

@testset "adjoint preserves Float32 element type (#4018, #4597)" begin
    v = Float32[1, 2]
    row = adjoint(v)
    @test eltype(row) === Float32
    @test size(row) == (1, 2)
    @test typeof(row[1, 1]) === Float32
    @test typeof(row[1, 2]) === Float32
    @test row[1, 1] == Float32(1)
    @test row[1, 2] == Float32(2)

    A = reshape(Float32[1, 2, 3, 4], 2, 2)
    transposed = adjoint(A)
    @test eltype(transposed) === Float32
    @test size(transposed) == (2, 2)
    @test typeof(transposed[1, 1]) === Float32
    @test typeof(transposed[1, 2]) === Float32
    @test typeof(transposed[2, 1]) === Float32
    @test typeof(transposed[2, 2]) === Float32
    @test transposed[1, 1] == Float32(1)
    @test transposed[1, 2] == Float32(2)
    @test transposed[2, 1] == Float32(3)
    @test transposed[2, 2] == Float32(4)
end
end # module Agg_adjoint_float32_type_preservation_4597

# ===== source: array/adjoint_small_integer_type_preservation_4602.jl =====
module Agg_adjoint_small_integer_type_preservation_4602
using Test

@testset "adjoint preserves small integer element types (#4018, #4602)" begin
    signed_row = adjoint(Int8[1, 2])
    @test eltype(signed_row) === Int8
    @test size(signed_row) == (1, 2)
    @test typeof(signed_row[1, 1]) === Int8
    @test signed_row[1, 1] == Int8(1)
    @test signed_row[1, 2] == Int8(2)

    unsigned_row = adjoint(UInt8[1, 2])
    @test eltype(unsigned_row) === UInt8
    @test size(unsigned_row) == (1, 2)
    @test typeof(unsigned_row[1, 1]) === UInt8
    @test unsigned_row[1, 1] == UInt8(1)
    @test unsigned_row[1, 2] == UInt8(2)

    signed_matrix = adjoint(reshape(Int8[1, 2, 3, 4], 2, 2))
    @test eltype(signed_matrix) === Int8
    @test size(signed_matrix) == (2, 2)
    @test typeof(signed_matrix[1, 2]) === Int8
    @test signed_matrix[1, 2] == Int8(2)
    @test signed_matrix[2, 1] == Int8(3)

    unsigned_matrix = adjoint(reshape(UInt8[1, 2, 3, 4], 2, 2))
    @test eltype(unsigned_matrix) === UInt8
    @test size(unsigned_matrix) == (2, 2)
    @test typeof(unsigned_matrix[1, 2]) === UInt8
    @test unsigned_matrix[1, 2] == UInt8(2)
    @test unsigned_matrix[2, 1] == UInt8(3)
end
end # module Agg_adjoint_small_integer_type_preservation_4602

# ===== source: array/cat_string_type_preservation_4592.jl =====
module Agg_cat_string_type_preservation_4592
using Test

@testset "cat String vectors preserves result eltype (#4018, #4592)" begin
    v = cat(["a", "b"], ["c"]; dims=1)
    @test typeof(v) === Vector{String}
    @test eltype(v) === String
    @test v == ["a", "b", "c"]
end

@testset "cat String matrices preserves result eltype (#4018, #4592)" begin
    A = ["a" "b"]
    B = ["c" "d"]

    vertical = cat(A, B; dims=1)
    @test typeof(vertical) === Matrix{String}
    @test eltype(vertical) === String
    @test size(vertical) == (2, 2)
    @test vertical[1, 1] == "a"
    @test vertical[1, 2] == "b"
    @test vertical[2, 1] == "c"
    @test vertical[2, 2] == "d"

    horizontal = cat(A, B; dims=2)
    @test typeof(horizontal) === Matrix{String}
    @test eltype(horizontal) === String
    @test size(horizontal) == (1, 4)
    @test horizontal[1, 1] == "a"
    @test horizontal[1, 2] == "b"
    @test horizontal[1, 3] == "c"
    @test horizontal[1, 4] == "d"
end

@testset "cat mixed eltypes promotes result eltype (#4018, #4651)" begin
    narrow = cat(Int8[1], Int16[2]; dims=1)
    @test typeof(narrow) === Vector{Int16}
    @test eltype(narrow) === Int16
    @test narrow == Int16[1, 2]

    floating = cat(Int8[1], Float32[2]; dims=1)
    @test typeof(floating) === Vector{Float32}
    @test eltype(floating) === Float32
    @test floating == Float32[1, 2]

    boxed = cat(String["a"], Any["b"]; dims=1)
    @test typeof(boxed) === Vector{Any}
    @test eltype(boxed) === Any
    @test boxed == Any["a", "b"]

    bool_int = cat(Bool[true], Int8[2]; dims=1)
    @test typeof(bool_int) === Vector{Int8}
    @test eltype(bool_int) === Int8
    @test bool_int == Int8[1, 2]
end
end # module Agg_cat_string_type_preservation_4592

# ===== source: array/concat_mixed_type_preservation_4655.jl =====
module Agg_concat_mixed_type_preservation_4655
using Test

@testset "hcat promotes mixed vector eltypes (#4018, #4655)" begin
    narrow = hcat(Int8[1, 2], Int16[3, 4])
    @test typeof(narrow) === Matrix{Int16}
    @test eltype(narrow) === Int16
    @test size(narrow) == (2, 2)
    @test typeof(narrow[1, 1]) === Int16
    @test narrow[1, 1] == Int16(1)
    @test narrow[1, 2] == Int16(3)
    @test narrow[2, 1] == Int16(2)
    @test narrow[2, 2] == Int16(4)

    floating = hcat(Int8[1, 2], Float32[3, 4])
    @test typeof(floating) === Matrix{Float32}
    @test eltype(floating) === Float32
    @test size(floating) == (2, 2)
    @test typeof(floating[1, 1]) === Float32
    @test floating[1, 1] == Float32(1)
    @test floating[1, 2] == Float32(3)
    @test floating[2, 1] == Float32(2)
    @test floating[2, 2] == Float32(4)

    boxed = hcat(String["a", "b"], Any["c", "d"])
    @test typeof(boxed) === Matrix{Any}
    @test eltype(boxed) === Any
    @test size(boxed) == (2, 2)
    @test boxed[1, 1] == "a"
    @test boxed[1, 2] == "c"
    @test boxed[2, 1] == "b"
    @test boxed[2, 2] == "d"
end

@testset "vcat promotes mixed vector eltypes (#4018, #4655)" begin
    narrow = vcat(Int8[1, 2], Int16[3, 4])
    @test typeof(narrow) === Vector{Int16}
    @test eltype(narrow) === Int16
    @test typeof(narrow[1]) === Int16
    @test narrow[1] == Int16(1)
    @test narrow[2] == Int16(2)
    @test narrow[3] == Int16(3)
    @test narrow[4] == Int16(4)

    floating = vcat(Int8[1, 2], Float32[3, 4])
    @test typeof(floating) === Vector{Float32}
    @test eltype(floating) === Float32
    @test typeof(floating[1]) === Float32
    @test floating[1] == Float32(1)
    @test floating[2] == Float32(2)
    @test floating[3] == Float32(3)
    @test floating[4] == Float32(4)

    boxed = vcat(String["a", "b"], Any["c", "d"])
    @test typeof(boxed) === Vector{Any}
    @test eltype(boxed) === Any
    @test boxed[1] == "a"
    @test boxed[2] == "b"
    @test boxed[3] == "c"
    @test boxed[4] == "d"
end
end # module Agg_concat_mixed_type_preservation_4655

# ===== source: array/cumulative_type_preservation_4018.jl =====
module Agg_cumulative_type_preservation_4018
using Test

@testset "cumsum preserves upstream result element types (#4018, #4590)" begin
    ints = cumsum([1, 2, 3])
    @test ints == [1, 3, 6]
    @test typeof(ints) === Vector{Int64}
    @test eltype(ints) === Int64

    narrow = cumsum(Int8[1, 2, 3])
    @test narrow == [1, 3, 6]
    @test typeof(narrow) === Vector{Int64}
    @test eltype(narrow) === Int64

    unsigned = cumsum(UInt8[1, 2, 3])
    @test unsigned[1] == UInt64(1)
    @test unsigned[2] == UInt64(3)
    @test unsigned[3] == UInt64(6)
    @test typeof(unsigned) === Vector{UInt64}
    @test eltype(unsigned) === UInt64

    floats32 = cumsum(Float32[1, 2, 3])
    @test floats32[1] === Float32(1)
    @test floats32[2] === Float32(3)
    @test floats32[3] === Float32(6)
    @test typeof(floats32) === Vector{Float32}
    @test eltype(floats32) === Float32

    bools = cumsum(Bool[true, false, true])
    @test bools == [1, 1, 2]
    @test typeof(bools) === Vector{Int64}
    @test eltype(bools) === Int64
end

@testset "cumprod preserves upstream result element types (#4018, #4590)" begin
    ints = cumprod([1, 2, 3])
    @test ints == [1, 2, 6]
    @test typeof(ints) === Vector{Int64}
    @test eltype(ints) === Int64

    narrow = cumprod(Int16[1, 2, 3])
    @test narrow == [1, 2, 6]
    @test typeof(narrow) === Vector{Int64}
    @test eltype(narrow) === Int64

    unsigned = cumprod(UInt16[1, 2, 3])
    @test unsigned[1] == UInt64(1)
    @test unsigned[2] == UInt64(2)
    @test unsigned[3] == UInt64(6)
    @test typeof(unsigned) === Vector{UInt64}
    @test eltype(unsigned) === UInt64

    floats32 = cumprod(Float32[1, 2, 3])
    @test floats32[1] === Float32(1)
    @test floats32[2] === Float32(2)
    @test floats32[3] === Float32(6)
    @test typeof(floats32) === Vector{Float32}
    @test eltype(floats32) === Float32

    bools = cumprod(Bool[true, false, true])
    @test bools == Bool[true, false, false]
    @test typeof(bools) === Vector{Bool}
    @test eltype(bools) === Bool
end
end # module Agg_cumulative_type_preservation_4018

# ===== source: array/empty_array_type_preservation_4599.jl =====
module Agg_empty_array_type_preservation_4599
using Test

@testset "empty array preserves requested element type (#4018, #4599)" begin
    float_empty = empty(Float32[1, 2])
    @test typeof(float_empty) === Vector{Float32}
    @test eltype(float_empty) === Float32
    @test length(float_empty) == 0

    int8_empty = empty(Int8[1, 2])
    @test typeof(int8_empty) === Vector{Int8}
    @test eltype(int8_empty) === Int8
    @test length(int8_empty) == 0

    requested_empty = empty(Float32[1, 2], Int16)
    @test typeof(requested_empty) === Vector{Int16}
    @test eltype(requested_empty) === Int16
    @test length(requested_empty) == 0

    any_empty = empty(Any[1, "x"])
    @test typeof(any_empty) === Vector{Any}
    @test eltype(any_empty) === Any
    @test length(any_empty) == 0
end
end # module Agg_empty_array_type_preservation_4599

# ===== source: array/hcat_small_integer_type_preservation_4607.jl =====
module Agg_hcat_small_integer_type_preservation_4607
using Test

@testset "hcat preserves small integer element types (#4018, #4607)" begin
    a16 = zeros(Int16, 2)
    b16 = zeros(Int16, 2)
    c16 = zeros(Int16, 2)
    a16[1] = 1
    a16[2] = 2
    b16[1] = 3
    b16[2] = 4
    c16[1] = 5
    c16[2] = 6
    r16 = hcat(a16, b16)
    @test typeof(r16) == Matrix{Int16}
    @test eltype(r16) == Int16
    @test typeof(r16[1, 1]) == Int16
    @test r16[1, 2] == Int16(3)
    r16v = hcat(a16, b16, c16)
    @test typeof(r16v) == Matrix{Int16}
    @test eltype(r16v) == Int16
    @test r16v[2, 3] == Int16(6)

    a32 = zeros(Int32, 2)
    b32 = zeros(Int32, 2)
    a32[1] = 1
    a32[2] = 2
    b32[1] = 3
    b32[2] = 4
    r32 = hcat(a32, b32)
    @test typeof(r32) == Matrix{Int32}
    @test eltype(r32) == Int32
    @test typeof(r32[1, 1]) == Int32
    @test r32[2, 2] == Int32(4)

    au8 = zeros(UInt8, 2)
    bu8 = zeros(UInt8, 2)
    au8[1] = 1
    au8[2] = 2
    bu8[1] = 3
    bu8[2] = 4
    ru8 = hcat(au8, bu8)
    @test typeof(ru8) == Matrix{UInt8}
    @test eltype(ru8) == UInt8
    @test typeof(ru8[1, 1]) == UInt8
    @test ru8[1, 2] == UInt8(3)

    au16 = zeros(UInt16, 2)
    bu16 = zeros(UInt16, 2)
    au16[1] = 1
    au16[2] = 2
    bu16[1] = 3
    bu16[2] = 4
    ru16 = hcat(au16, bu16)
    @test typeof(ru16) == Matrix{UInt16}
    @test eltype(ru16) == UInt16
    @test typeof(ru16[1, 1]) == UInt16
    @test ru16[2, 2] == UInt16(4)

    au32 = zeros(UInt32, 2)
    bu32 = zeros(UInt32, 2)
    au32[1] = 1
    au32[2] = 2
    bu32[1] = 3
    bu32[2] = 4
    ru32 = hcat(au32, bu32)
    @test typeof(ru32) == Matrix{UInt32}
    @test eltype(ru32) == UInt32
    @test typeof(ru32[1, 1]) == UInt32
    @test ru32[1, 2] == UInt32(3)

    au64 = zeros(UInt64, 2)
    bu64 = zeros(UInt64, 2)
    au64[1] = 1
    au64[2] = 2
    bu64[1] = 3
    bu64[2] = 4
    ru64 = hcat(au64, bu64)
    @test typeof(ru64) == Matrix{UInt64}
    @test eltype(ru64) == UInt64
    @test typeof(ru64[1, 1]) == UInt64
    @test ru64[2, 2] == UInt64(4)
end
end # module Agg_hcat_small_integer_type_preservation_4607

# ===== source: array/hcat_type_preservation.jl =====
module Agg_hcat_type_preservation
using Test

# Regression test for Issue #3588:
# `hcat([1, 2], [3, 4])` previously returned `Matrix{Float64}` because
# the implementation pre-allocated `result = zeros(na, 2)`. Per the
# #3588 acceptance criteria, the result element type must match the
# input for the 2-argument case.
#
# Implementation: dispatch-based specialization on the common Vector{T}
# types (Int64/Float64/Bool/String/Char) seeds a typed empty `T[]` and
# reshapes after push! accumulation so the returned `Matrix{T}` matches
# the input element type. Generic 1-argument and 4+-argument fallbacks
# return `Matrix{Any}` because typed varargs tie with the generic
# `hcat(args...)` during dispatch (no AmbiguousMethod tie-breaker for
# two varargs candidates), and pure-Julia `similar` inside a function
# is blocked by Issue #3648.

@testset "hcat preserves Matrix{Int64} (#3588)" begin
    m = hcat([1, 2], [3, 4])
    @test size(m) == (2, 2)
    @test m == [1 3; 2 4]
    @test typeof(m) === Matrix{Int64}

    # 3-argument case also preserved
    m3 = hcat([1, 2], [3, 4], [5, 6])
    @test size(m3) == (2, 3)
    @test m3 == [1 3 5; 2 4 6]
    @test typeof(m3) === Matrix{Int64}
end

@testset "hcat preserves Matrix{Bool}" begin
    m = hcat([true, false], [false, true])
    @test size(m) == (2, 2)
    @test m[1, 1] == true
    @test m[2, 2] == true
    @test m[1, 2] == false
    @test typeof(m) === Matrix{Bool}
end

@testset "hcat preserves Matrix{Float64} (regression)" begin
    m = hcat([1.0, 2.0], [3.0, 4.0])
    @test size(m) == (2, 2)
    @test m == [1.0 3.0; 2.0 4.0]
    @test typeof(m) === Matrix{Float64}
end

@testset "hcat preserves Matrix{String}" begin
    m = hcat(["a", "b"], ["c", "d"])
    @test size(m) == (2, 2)
    @test m[1, 1] == "a"
    @test m[2, 2] == "d"
    @test typeof(m) === Matrix{String}
end

@testset "hcat dimension mismatch still raises" begin
    @test_throws Exception hcat([1, 2], [3, 4, 5])
end
end # module Agg_hcat_type_preservation

# ===== source: array/hcat_varargs_type_preservation_4280.jl =====
module Agg_hcat_varargs_type_preservation_4280
using Test

@testset "hcat 4+ typed vectors preserves element type (Issue #4280)" begin
    mi = hcat([1], [2], [3], [4])
    @test typeof(mi) == Matrix{Int64}
    @test size(mi) == (1, 4)
    @test mi[1, 4] == 4

    mi5 = hcat([1], [2], [3], [4], [5])
    @test typeof(mi5) == Matrix{Int64}
    @test size(mi5) == (1, 5)
    @test mi5[1, 5] == 5

    mi8 = hcat(Int8[1], Int8[2], Int8[3], Int8[4])
    @test typeof(mi8) == Matrix{Int8}
    @test eltype(mi8) == Int8
    @test mi8[1, 4] == Int8(4)

    mf = hcat([1.0], [2.0], [3.0], [4.0])
    @test typeof(mf) == Matrix{Float64}

    mf32 = hcat(Float32[1], Float32[2], Float32[3], Float32[4])
    @test typeof(mf32) == Matrix{Float32}
    @test eltype(mf32) == Float32
    @test mf32[1, 4] == Float32(4)

    mb = hcat([true], [false], [true], [false])
    @test typeof(mb) == Matrix{Bool}

    promoted = hcat([1], [2.0], [3], [4.0])
    @test typeof(promoted) == Matrix{Float64}
    @test eltype(promoted) == Float64
    @test promoted[1, 1] == 1.0
    @test promoted[1, 4] == 4.0

    promoted2 = hcat([1], [2.0])
    @test typeof(promoted2) == Matrix{Float64}
    @test eltype(promoted2) == Float64
    @test promoted2[1, 1] == 1.0

    mi8_2 = hcat(Int8[1], Int8[2])
    @test typeof(mi8_2) == Matrix{Int8}

    mixed = hcat([1], [2], [3], [4], ["x"])
    @test typeof(mixed) == Matrix{Any}
end
end # module Agg_hcat_varargs_type_preservation_4280

# ===== source: array/literal_native_finalize_elemtypes_6846.jl =====
module Agg_literal_native_finalize_elemtypes_6846
# Regression guard for Issue #6846.
#
# Array literals now finalize their backing `Memory` into the `Array{T,N}`
# wrapper natively (a zero-copy `MemoryRef` view) instead of calling the
# per-literal pure-Julia `wrap(::Type{Array}, mem, dims)`. The native finalize
# must reconstruct every element-type storage layout correctly — interleaved
# `Complex`, array-of-struct (AoS), boxed `Any`, plain primitives, multi-dim —
# which the earlier `ArrayValue` round-trip mishandled for the non-primitive
# layouts (it produced an out-of-bounds wrapper).

using Test

struct Pt
    x::Int
    y::Int
end

@testset "array literal native finalize across element types (Issue #6846)" begin
    # plain primitives
    ai = [1, 2, 3]
    @test length(ai) == 3
    @test ai[2] == 2
    af = [1.0, 2.0]
    @test af[1] == 1.0
    ab = [true, false, true]
    @test ab[3] == true
    astr = ["a", "b"]
    @test astr[2] == "b"
    ac = ['x', 'y']
    @test ac[1] == 'x'

    # mixed -> Vector{Any}
    aa = [1, "two", 3.0]
    @test length(aa) == 3
    @test aa[2] == "two"

    # interleaved Complex (this was the regression: out-of-bounds on index)
    ax = [Complex(1.0, 2.0), Complex(3.0, 4.0)]
    @test ax isa Vector{Complex{Float64}}
    @test length(ax) == 2
    @test real(ax[1]) == 1.0
    @test imag(ax[2]) == 4.0
    axi = [1.0 + 2.0im, 3.0 + 4.0im]
    @test imag(axi[1]) == 2.0

    # array-of-struct (AoS)
    ap = [Pt(1, 2), Pt(3, 4)]
    @test length(ap) == 2
    @test ap[1].x == 1
    @test ap[2].y == 4

    # multi-dim literal (column-major)
    m = [1 2; 3 4]
    @test size(m) == (2, 2)
    @test m[2, 1] == 3
    @test m[1, 2] == 2

    # empty typed literal
    e = Int[]
    @test e isa Vector{Int}
    @test length(e) == 0
end
end # module Agg_literal_native_finalize_elemtypes_6846

# ===== source: array/mapslices_string_type_preservation_4593.jl =====
module Agg_mapslices_string_type_preservation_4593
using Test

@testset "mapslices String matrix allocates typed slices and Int result (#4018, #4593)" begin
    A = ["a" "bb" "ccc"; "dddd" "eeeee" "ffffff"]

    by_column = mapslices(length, A; dims=1)
    @test typeof(by_column) === Matrix{Int64}
    @test eltype(by_column) === Int64
    @test size(by_column) == (1, 3)
    @test by_column[1, 1] == 2
    @test by_column[1, 2] == 2
    @test by_column[1, 3] == 2

    by_row = mapslices(length, A; dims=2)
    @test typeof(by_row) === Matrix{Int64}
    @test eltype(by_row) === Int64
    @test size(by_row) == (2, 1)
    @test by_row[1, 1] == 3
    @test by_row[2, 1] == 3
end
end # module Agg_mapslices_string_type_preservation_4593

# ===== source: array/permutedims_explicit_float32_type_preservation_4598.jl =====
module Agg_permutedims_explicit_float32_type_preservation_4598
using Test

@testset "explicit permutedims preserves Float32 element type (#4018, #4598)" begin
    A = reshape(Float32[1, 2, 3, 4], 2, 2)
    identity_copy = permutedims(A, (1, 2))
    @test typeof(identity_copy) === Matrix{Float32}
    @test eltype(identity_copy) === Float32
    @test size(identity_copy) == (2, 2)
    @test typeof(identity_copy[1, 1]) === Float32
    @test identity_copy[1, 1] == Float32(1)
    @test identity_copy[2, 2] == Float32(4)

    B = reshape(Float32[1, 2, 3, 4, 5, 6, 7, 8], 2, 2, 2)
    permuted3 = permutedims(B, (2, 1, 3))
    @test typeof(permuted3) === Array{Float32,3}
    @test eltype(permuted3) === Float32
    @test size(permuted3) == (2, 2, 2)
    @test typeof(permuted3[1, 1, 1]) === Float32
    @test permuted3[1, 1, 1] == Float32(1)
    @test permuted3[1, 2, 1] == Float32(2)
    @test permuted3[2, 1, 2] == Float32(7)

    C = reshape(Float32[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16], 2, 2, 2, 2)
    permuted4 = permutedims(C, (2, 1, 3, 4))
    @test typeof(permuted4) === Array{Float32,4}
    @test eltype(permuted4) === Float32
    @test size(permuted4) == (2, 2, 2, 2)
    @test typeof(permuted4[1, 1, 1, 1]) === Float32
    @test permuted4[1, 1, 1, 1] == Float32(1)
    @test permuted4[1, 2, 1, 1] == Float32(2)
    @test permuted4[2, 1, 2, 2] == Float32(15)
end
end # module Agg_permutedims_explicit_float32_type_preservation_4598

# ===== source: array/permutedims_type_preservation.jl =====
module Agg_permutedims_type_preservation
using Test

# Regression test for Issue #3590:
# `permutedims([true, false])` previously returned `Matrix{Float64}`
# because the implementation only special-cased `T == Int64` and fell
# back to `zeros(...)` (Float64) for everything else. Per the #3590
# acceptance criteria, the result must preserve the input element type.
#
# Result allocation now uses `similar(arr, ...)`, so narrow element types
# are preserved without a flat `Any[]` fallback.

@testset "permutedims preserves Matrix{Bool} (#3590)" begin
    x = permutedims([true, false])
    @test size(x) == (1, 2)
    @test x[1, 1] == true
    @test x[1, 2] == false
    @test typeof(x) === Matrix{Bool}
end

@testset "permutedims preserves Matrix{Int64} (1D)" begin
    x = permutedims([1, 2, 3])
    @test size(x) == (1, 3)
    @test x == [1 2 3]
    @test typeof(x) === Matrix{Int64}
end

@testset "permutedims preserves Matrix{Float64} (regression)" begin
    x = permutedims([1.0, 2.0])
    @test size(x) == (1, 2)
    @test x == [1.0 2.0]
    @test typeof(x) === Matrix{Float64}
end

@testset "permutedims preserves String values" begin
    x = permutedims(["a", "b"])
    @test size(x) == (1, 2)
    @test x[1, 1] == "a"
    @test x[1, 2] == "b"
    @test typeof(x) === Matrix{String}
end

@testset "permutedims 2D transpose preserves element type" begin
    m = [1 2; 3 4]
    t = permutedims(m)
    @test size(t) == (2, 2)
    @test t == [1 3; 2 4]
    @test typeof(t) === Matrix{Int64}

    mb = [true false; false true]
    tb = permutedims(mb)
    @test tb == [true false; false true]
    @test typeof(tb) === Matrix{Bool}

    mf = [1.0 2.0; 3.0 4.0]
    tf = permutedims(mf)
    @test tf == [1.0 3.0; 2.0 4.0]
    @test typeof(tf) === Matrix{Float64}
end

@testset "permutedims preserves narrow and Float32 element types (#4018, #4656)" begin
    v8 = permutedims(Int8[1, 2])
    @test typeof(v8) === Matrix{Int8}
    @test eltype(v8) === Int8
    @test size(v8) == (1, 2)
    @test typeof(v8[1, 1]) === Int8
    @test v8[1, 1] == Int8(1)
    @test v8[1, 2] == Int8(2)

    m8 = permutedims(reshape(Int8[1, 2, 3, 4], 2, 2))
    @test typeof(m8) === Matrix{Int8}
    @test eltype(m8) === Int8
    @test size(m8) == (2, 2)
    @test typeof(m8[1, 1]) === Int8
    @test m8[1, 1] == Int8(1)
    @test m8[1, 2] == Int8(2)
    @test m8[2, 1] == Int8(3)
    @test m8[2, 2] == Int8(4)

    vf32 = permutedims(Float32[1, 2])
    @test typeof(vf32) === Matrix{Float32}
    @test eltype(vf32) === Float32
    @test size(vf32) == (1, 2)
    @test typeof(vf32[1, 1]) === Float32
    @test vf32[1, 1] == Float32(1)
    @test vf32[1, 2] == Float32(2)

    mf32 = permutedims(reshape(Float32[1, 2, 3, 4], 2, 2))
    @test typeof(mf32) === Matrix{Float32}
    @test eltype(mf32) === Float32
    @test size(mf32) == (2, 2)
    @test typeof(mf32[1, 1]) === Float32
    @test mf32[1, 1] == Float32(1)
    @test mf32[1, 2] == Float32(2)
    @test mf32[2, 1] == Float32(3)
    @test mf32[2, 2] == Float32(4)
end
end # module Agg_permutedims_type_preservation

# ===== source: array/prod_bang_type_preservation_4616.jl =====
module Agg_prod_bang_type_preservation_4616
using Test

@testset "prod! preserves typed in-place reduction semantics (#4019, #4616)" begin
    words = reshape(String["a", "c", "b", "d"], 2, 2)

    word_cols = similar(words, 1, 2)
    returned_cols = prod!(word_cols, words)
    @test returned_cols === word_cols
    @test typeof(word_cols) == Matrix{String}
    @test eltype(word_cols) == String
    @test word_cols[1, 1] == "ac"
    @test word_cols[1, 2] == "bd"

    word_rows = Vector{String}(undef, 2)
    returned_rows = prod!(word_rows, words)
    @test returned_rows === word_rows
    @test typeof(word_rows) == Vector{String}
    @test eltype(word_rows) == String
    @test word_rows[1] == "ab"
    @test word_rows[2] == "cd"

    flags = reshape(Bool[true, true, false, true], 2, 2)
    bool_cols = similar(flags, 1, 2)
    prod!(bool_cols, flags)
    @test typeof(bool_cols) == Matrix{Bool}
    @test eltype(bool_cols) == Bool
    @test bool_cols[1, 1] == true
    @test bool_cols[1, 2] == false

    narrow = reshape(Int8[1, 4, 2, 5], 2, 2)
    int_cols = zeros(Int64, 1, 2)
    prod!(int_cols, narrow)
    @test typeof(int_cols) == Matrix{Int64}
    @test eltype(int_cols) == Int64
    @test int_cols[1, 1] == 4
    @test int_cols[1, 2] == 10
end
end # module Agg_prod_bang_type_preservation_4616

# ===== source: array/prod_dims_type_preservation_4614.jl =====
module Agg_prod_dims_type_preservation_4614
using Test

@testset "prod dims preserves upstream reduction result types (#4019, #4614)" begin
    narrow = reshape(Int8[1, 4, 2, 5, 3, 6], 2, 3)

    cprod = prod(narrow; dims=1)
    @test typeof(cprod) == Matrix{Int64}
    @test eltype(cprod) == Int64
    @test typeof(cprod[1]) == Int64
    @test size(cprod) == (1, 3)
    @test cprod[1, 1] == 4
    @test cprod[1, 2] == 10
    @test cprod[1, 3] == 18

    rprod = prod(narrow; dims=2)
    @test typeof(rprod) == Matrix{Int64}
    @test eltype(rprod) == Int64
    @test typeof(rprod[1]) == Int64
    @test size(rprod) == (2, 1)
    @test rprod[1, 1] == 6
    @test rprod[2, 1] == 120

    words = reshape(String["a", "c", "b", "d"], 2, 2)

    word_cols = prod(words; dims=1)
    @test typeof(word_cols) == Matrix{String}
    @test eltype(word_cols) == String
    @test typeof(word_cols[1]) == String
    @test size(word_cols) == (1, 2)
    @test word_cols[1, 1] == "ac"
    @test word_cols[1, 2] == "bd"

    word_rows = prod(words; dims=2)
    @test typeof(word_rows) == Matrix{String}
    @test eltype(word_rows) == String
    @test typeof(word_rows[1]) == String
    @test size(word_rows) == (2, 1)
    @test word_rows[1, 1] == "ab"
    @test word_rows[2, 1] == "cd"

    flags = reshape(Bool[true, true, false, true], 2, 2)
    bool_cols = prod(flags; dims=1)
    @test typeof(bool_cols) == Matrix{Bool}
    @test eltype(bool_cols) == Bool
    @test typeof(bool_cols[1]) == Bool
    @test size(bool_cols) == (1, 2)
    @test bool_cols[1, 1] == true
    @test bool_cols[1, 2] == false
end
end # module Agg_prod_dims_type_preservation_4614

# ===== source: array/prod_scalar_type_preservation_4615.jl =====
module Agg_prod_scalar_type_preservation_4615
using Test

@testset "prod scalar preserves upstream reduction result types (#4019, #4615)" begin
    signed = prod(Int8[2, 3])
    @test typeof(signed) == Int64
    @test signed == 6

    unsigned = prod(UInt8[2, 3])
    @test typeof(unsigned) == UInt64
    @test unsigned == UInt64(6)

    floats = prod(Float32[2, 3])
    @test typeof(floats) == Float32
    @test floats == Float32(6)

    bools = prod(Bool[true, false])
    @test typeof(bools) == Bool
    @test bools == false

    words = prod(String["a", "b"])
    @test typeof(words) == String
    @test words == "ab"

    empty_unsigned = prod(UInt8[])
    @test typeof(empty_unsigned) == UInt64
    @test empty_unsigned == UInt64(1)

    empty_float = prod(Float32[])
    @test typeof(empty_float) == Float32
    @test empty_float == Float32(1)

    empty_bool = prod(Bool[])
    @test typeof(empty_bool) == Bool
    @test empty_bool == true

    empty_words = prod(String[])
    @test typeof(empty_words) == String
    @test empty_words == ""
end
end # module Agg_prod_scalar_type_preservation_4615

# ===== source: array/reduce_dims_string_type_preservation_4595.jl =====
module Agg_reduce_dims_string_type_preservation_4595
using Test

@testset "reduction dims String matrix results preserve Julia result types (#4018, #4595)" begin
    A = ["b" "a"; "d" "c"]

    min_cols = minimum(A; dims=1)
    @test typeof(min_cols) === Matrix{String}
    @test eltype(min_cols) === String
    @test size(min_cols) == (1, 2)
    @test min_cols[1, 1] == "b"
    @test min_cols[1, 2] == "a"

    min_rows = minimum(A; dims=2)
    @test typeof(min_rows) === Matrix{String}
    @test eltype(min_rows) === String
    @test size(min_rows) == (2, 1)
    @test min_rows[1, 1] == "a"
    @test min_rows[2, 1] == "c"

    max_cols = maximum(A; dims=1)
    @test typeof(max_cols) === Matrix{String}
    @test eltype(max_cols) === String
    @test size(max_cols) == (1, 2)
    @test max_cols[1, 1] == "d"
    @test max_cols[1, 2] == "c"

    max_rows = maximum(A; dims=2)
    @test typeof(max_rows) === Matrix{String}
    @test eltype(max_rows) === String
    @test size(max_rows) == (2, 1)
    @test max_rows[1, 1] == "b"
    @test max_rows[2, 1] == "d"

    extrema_cols = extrema(A; dims=1)
    @test typeof(extrema_cols) === Matrix{Tuple{String,String}}
    @test eltype(extrema_cols) === Tuple{String,String}
    @test size(extrema_cols) == (1, 2)
    @test extrema_cols[1, 1] == ("b", "d")
    @test extrema_cols[1, 2] == ("a", "c")

    extrema_rows = extrema(A; dims=2)
    @test typeof(extrema_rows) === Matrix{Tuple{String,String}}
    @test eltype(extrema_rows) === Tuple{String,String}
    @test size(extrema_rows) == (2, 1)
    @test extrema_rows[1, 1] == ("a", "b")
    @test extrema_rows[2, 1] == ("c", "d")
end
end # module Agg_reduce_dims_string_type_preservation_4595

# ===== source: array/rotation_type_preservation.jl =====
module Agg_rotation_type_preservation
using Test

# Regression test for Issue #3589:
# `rotl90([1 2; 3 4])` previously returned `Matrix{Float64}` because the
# implementation pre-allocated `result = zeros(n, m)`. The same defect
# affected `rotr90` and `rot180`. Per the #3589 acceptance criteria, all
# three should preserve the element type.
#
# Implementation: dispatch-based specialization. Matrix literals like
# `[1 2; 3 4]` infer as `Vector{T}` at compile time even though the
# runtime type is `Matrix{T}`, so methods are declared on `Vector{T}`
# and seed the flat buffer with `T[]`. push! + reshape preserves the
# element type. Generic fallback returns `Matrix{Any}` because pure-Julia
# `similar(mat, n, m)` is blocked by Issue #3648.

@testset "rotl90 preserves Matrix{Int64} (#3589)" begin
    m = [1 2; 3 4]
    r = rotl90(m)
    @test r == [2 4; 1 3]
    @test typeof(r) === Matrix{Int64}

    # Non-square 3x4
    m3 = [1 2 3 4; 5 6 7 8; 9 10 11 12]
    r3 = rotl90(m3)
    @test r3 == [4 8 12; 3 7 11; 2 6 10; 1 5 9]
    @test typeof(r3) === Matrix{Int64}
    @test size(r3) == (4, 3)
end

@testset "rotr90 preserves Matrix{Int64}" begin
    m = [1 2; 3 4]
    r = rotr90(m)
    @test r == [3 1; 4 2]
    @test typeof(r) === Matrix{Int64}
end

@testset "rot180 preserves Matrix{Int64}" begin
    m = [1 2; 3 4]
    r = rot180(m)
    @test r == [4 3; 2 1]
    @test typeof(r) === Matrix{Int64}
end

@testset "rotl90 preserves Matrix{Bool}" begin
    m = [true false; false true]
    r = rotl90(m)
    @test r == [false true; true false]
    @test typeof(r) === Matrix{Bool}
end

@testset "rotation regressions for Matrix{Float64}" begin
    m = [1.0 2.0; 3.0 4.0]
    r1 = rotl90(m)
    r2 = rotr90(m)
    r3 = rot180(m)
    @test r1[1, 1] == 2.0
    @test r2[1, 1] == 3.0
    @test r3[1, 1] == 4.0
    @test typeof(r1) === Matrix{Float64}
    @test typeof(r2) === Matrix{Float64}
    @test typeof(r3) === Matrix{Float64}
end

@testset "rot composition still cancels (regression)" begin
    m = [1 2; 3 4]
    @test rotr90(rotl90(m)) == m
    @test rot180(rot180(m)) == m
end
end # module Agg_rotation_type_preservation

# ===== source: array/sortslices_string_type_preservation_4594.jl =====
module Agg_sortslices_string_type_preservation_4594
using Test

@testset "sortslices String matrix preserves result eltype (#4018, #4594)" begin
    rows = ["b" "x"; "a" "y"]
    sorted_rows = sortslices(rows; dims=1)
    @test typeof(sorted_rows) === Matrix{String}
    @test eltype(sorted_rows) === String
    @test size(sorted_rows) == (2, 2)
    @test sorted_rows[1, 1] == "a"
    @test sorted_rows[1, 2] == "y"
    @test sorted_rows[2, 1] == "b"
    @test sorted_rows[2, 2] == "x"

    cols = ["b" "a"; "x" "y"]
    sorted_cols = sortslices(cols; dims=2)
    @test typeof(sorted_cols) === Matrix{String}
    @test eltype(sorted_cols) === String
    @test size(sorted_cols) == (2, 2)
    @test sorted_cols[1, 1] == "a"
    @test sorted_cols[1, 2] == "b"
    @test sorted_cols[2, 1] == "y"
    @test sorted_cols[2, 2] == "x"
end
end # module Agg_sortslices_string_type_preservation_4594

# ===== source: array/stack_type_preservation.jl =====
module Agg_stack_type_preservation
using Test

# Regression test for Issue #3591:
# `stack(arrays)` previously hard-coded `Matrix{Float64}` output via
# `zeros(m, n)`, widening any non-Float input. Homogeneous input now allocates
# through similar(first_arr, m, n), preserving container element type.

@testset "stack preserves Int values (#3591)" begin
    x = stack([[1, 2], [3, 4]])
    @test typeof(x) === Matrix{Int64}
    @test eltype(x) === Int64
    @test size(x) == (2, 2)
    @test x[1, 1] == 1
    @test x[2, 1] == 2
    @test x[1, 2] == 3
    @test x[2, 2] == 4
    # Element value equality holds (matrix-level == checks each element)
    @test x == [1 3; 2 4]
end

@testset "stack preserves narrow numeric container types (#4018, #4603)" begin
    f32 = stack([Float32[1, 2], Float32[3, 4]])
    @test typeof(f32) === Matrix{Float32}
    @test eltype(f32) === Float32
    @test size(f32) == (2, 2)
    @test typeof(f32[1, 1]) === Float32
    @test f32[1, 1] == Float32(1)
    @test f32[2, 2] == Float32(4)

    i8 = stack([Int8[1, 2], Int8[3, 4]])
    @test typeof(i8) === Matrix{Int8}
    @test eltype(i8) === Int8
    @test size(i8) == (2, 2)
    @test typeof(i8[1, 1]) === Int8
    @test i8[1, 1] == Int8(1)
    @test i8[2, 2] == Int8(4)
end

@testset "stack promotes mixed input eltypes (#4018, #4652)" begin
    narrow = stack((Int8[1, 2], Int16[3, 4]))
    @test typeof(narrow) === Matrix{Int16}
    @test eltype(narrow) === Int16
    @test size(narrow) == (2, 2)
    @test typeof(narrow[1, 1]) === Int16
    @test narrow[1, 1] == Int16(1)
    @test narrow[1, 2] == Int16(3)
    @test narrow[2, 1] == Int16(2)
    @test narrow[2, 2] == Int16(4)

    floating = stack((Int8[1, 2], Float32[3, 4]))
    @test typeof(floating) === Matrix{Float32}
    @test eltype(floating) === Float32
    @test size(floating) == (2, 2)
    @test typeof(floating[1, 1]) === Float32
    @test floating[1, 1] == Float32(1)
    @test floating[1, 2] == Float32(3)
    @test floating[2, 1] == Float32(2)
    @test floating[2, 2] == Float32(4)

    boxed = stack((String["a", "b"], Any["c", "d"]))
    @test typeof(boxed) === Matrix{Any}
    @test eltype(boxed) === Any
    @test size(boxed) == (2, 2)
    @test boxed[1, 1] == "a"
    @test boxed[1, 2] == "c"
    @test boxed[2, 1] == "b"
    @test boxed[2, 2] == "d"
end

@testset "stack preserves Bool values" begin
    x = stack([[true, false], [false, true]])
    @test typeof(x) === Matrix{Bool}
    @test eltype(x) === Bool
    @test size(x) == (2, 2)
    @test x[1, 1] == true
    @test x[2, 2] == true
    @test x[1, 2] == false
end

@testset "stack preserves String values" begin
    x = stack([["a", "b"], ["c", "d"]])
    @test typeof(x) === Matrix{String}
    @test eltype(x) === String
    @test size(x) == (2, 2)
    @test x[1, 1] == "a"
    @test x[2, 2] == "d"
end

@testset "stack regression for Float64" begin
    x = stack([[1.0, 2.0], [3.0, 4.0]])
    @test typeof(x) === Matrix{Float64}
    @test eltype(x) === Float64
    @test size(x) == (2, 2)
    @test x[1, 1] == 1.0
    @test x[2, 2] == 4.0
end

@testset "stack edge cases" begin
    # Single column
    x = stack([[1, 2, 3]])
    @test size(x) == (3, 1)
    @test x[1, 1] == 1
    @test x[3, 1] == 3

    # Single-element columns
    y = stack([[1], [2], [3]])
    @test size(y) == (1, 3)
    @test y[1, 1] == 1
    @test y[1, 3] == 3
end
end # module Agg_stack_type_preservation

# ===== source: array/string_literal_type_preservation_4277.jl =====
module Agg_string_literal_type_preservation_4277
using Test

@testset "string array literals preserve Vector{String} (Issue #4277)" begin
    xs = ["a", "b", "c"]
    @test typeof(xs) === Vector{String}
    @test eltype(xs) === String
    @test xs == ["a", "b", "c"]

    typed_xs = String["a", "b"]
    @test typeof(typed_xs) === Vector{String}
    @test eltype(typed_xs) === String
    @test typed_xs == ["a", "b"]

    ys = ['a', 'b']
    @test typeof(ys) === Vector{Char}
    @test eltype(ys) === Char

    typed_ys = Char['x', 'y']
    @test typeof(typed_ys) === Vector{Char}
    @test eltype(typed_ys) === Char
    @test typed_ys == ['x', 'y']

    narrow_ints = Int8[1, 2]
    @test typeof(narrow_ints) === Vector{Int8}
    @test eltype(narrow_ints) === Int8

    mixed_any = Any["a", 1]
    @test typeof(mixed_any) === Vector{Any}
    @test eltype(mixed_any) === Any
    @test mixed_any == Any["a", 1]

    explicit_getindex = getindex(String, "a", "b")
    @test typeof(explicit_getindex) === Vector{String}
    @test explicit_getindex == ["a", "b"]
end
end # module Agg_string_literal_type_preservation_4277

# ===== source: array/sum_bang_type_preservation_4617.jl =====
module Agg_sum_bang_type_preservation_4617
using Test

@testset "sum! preserves typed in-place reduction semantics (#4019, #4617)" begin
    flags = reshape(Bool[true, true, false, true], 2, 2)

    int_cols = zeros(Int64, 1, 2)
    returned_cols = sum!(int_cols, flags)
    @test returned_cols === int_cols
    @test typeof(int_cols) == Matrix{Int64}
    @test eltype(int_cols) == Int64
    @test int_cols[1, 1] == 2
    @test int_cols[1, 2] == 1

    bool_ok_src = reshape(Bool[true, false, false, false], 2, 2)
    bool_ok = similar(bool_ok_src, 1, 2)
    returned_bool = sum!(bool_ok, bool_ok_src)
    @test returned_bool === bool_ok
    @test typeof(bool_ok) == Matrix{Bool}
    @test eltype(bool_ok) == Bool
    @test bool_ok[1, 1] == true
    @test bool_ok[1, 2] == false

    bool_bad = similar(flags, 1, 2)
    @test_throws Exception sum!(bool_bad, flags)

    narrow = reshape(Int8[1, 4, 2, 5], 2, 2)
    narrow_cols = similar(narrow, 1, 2)
    sum!(narrow_cols, narrow)
    @test typeof(narrow_cols) == Matrix{Int8}
    @test eltype(narrow_cols) == Int8
    @test narrow_cols[1, 1] == Int8(5)
    @test narrow_cols[1, 2] == Int8(7)

    narrow_rows = zeros(Int64, 2)
    sum!(narrow_rows, narrow)
    @test typeof(narrow_rows) == Vector{Int64}
    @test eltype(narrow_rows) == Int64
    @test narrow_rows[1] == 3
    @test narrow_rows[2] == 9
end
end # module Agg_sum_bang_type_preservation_4617

# ===== source: array/sum_dims_bool_type_preservation_4596.jl =====
module Agg_sum_dims_bool_type_preservation_4596
using Test

@testset "sum Bool matrix dims returns Int64 result (#4018, #4596)" begin
    A = [true false; true true]

    by_column = sum(A; dims=1)
    @test typeof(by_column) === Matrix{Int64}
    @test eltype(by_column) === Int64
    @test size(by_column) == (1, 2)
    @test by_column[1, 1] == 2
    @test by_column[1, 2] == 1

    by_row = sum(A; dims=2)
    @test typeof(by_row) === Matrix{Int64}
    @test eltype(by_row) === Int64
    @test size(by_row) == (2, 1)
    @test by_row[1, 1] == 1
    @test by_row[2, 1] == 2
end
end # module Agg_sum_dims_bool_type_preservation_4596

# ===== source: array/sum_scalar_type_preservation_4618.jl =====
module Agg_sum_scalar_type_preservation_4618
using Test

@testset "sum scalar preserves upstream reduction result types (#4019, #4618)" begin
    bools = sum(Bool[true, false, true])
    @test typeof(bools) == Int64
    @test bools == 2

    empty_bools = sum(Bool[])
    @test typeof(empty_bools) == Int64
    @test empty_bools == 0

    signed = sum(Int8[1, 2, 3])
    @test typeof(signed) == Int64
    @test signed == 6

    empty_signed = sum(Int8[])
    @test typeof(empty_signed) == Int64
    @test empty_signed == 0

    unsigned = sum(UInt8[1, 2, 3])
    @test typeof(unsigned) == UInt64
    @test unsigned == UInt64(6)

    empty_unsigned = sum(UInt8[])
    @test typeof(empty_unsigned) == UInt64
    @test empty_unsigned == UInt64(0)

    floats = sum(Float32[1, 2, 3])
    @test typeof(floats) == Float32
    @test floats == Float32(6)

    empty_floats = sum(Float32[])
    @test typeof(empty_floats) == Float32
    @test empty_floats == Float32(0)
end
end # module Agg_sum_scalar_type_preservation_4618

# ===== source: array/zero_preserves_element_type_4419.jl =====
module Agg_zero_preserves_element_type_4419
using Test

@testset "zero(::Array) preserves element type (Issue #4419)" begin
    ints = zero([1, 2, 3])
    @test typeof(ints) === Vector{Int64}
    @test ints[1] === Int64(0)
    @test ints[2] === Int64(0)
    @test ints[3] === Int64(0)

    bools = zero([true, false])
    @test typeof(bools) === Vector{Bool}
    @test bools[1] == false
    @test bools[2] == false

    floats = zero([1.0 2.0; 3.0 4.0])
    @test typeof(floats) === Matrix{Float64}
    @test size(floats) === (2, 2)
    @test floats[1, 1] === 0.0
    @test floats[2, 2] === 0.0
end
end # module Agg_zero_preserves_element_type_4419

true
