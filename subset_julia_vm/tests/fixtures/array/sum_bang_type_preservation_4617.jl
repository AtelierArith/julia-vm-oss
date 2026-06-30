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

true
