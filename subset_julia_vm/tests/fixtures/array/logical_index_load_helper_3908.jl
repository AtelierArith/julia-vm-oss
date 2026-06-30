using Test

@testset "logical IndexLoad reads target through ArrayValue helper (Issue #3908)" begin
    # Logical (Bool) indexing on an Int64 vector exercises the
    # load_selected_array_elements path that now reads each selected
    # element via ArrayValue::get_linear instead of the multi-dim get,
    # so reshape-aware shared-backing semantics stay intact.
    data = [10, 20, 30, 40, 50]
    mask = [true, false, true, false, true]

    picked = data[mask]
    @test picked == [10, 30, 50]
    @test typeof(picked) == Vector{Int64}

    # Integer-index array selection through the same helper.
    integer_idx = [5, 1, 3]
    by_index = data[integer_idx]
    @test by_index == [50, 10, 30]
    @test typeof(by_index) == Vector{Int64}

    # Float64 selection confirms element type is preserved when the helper
    # reads logical f64 elements one at a time.
    float_data = [1.5, 2.5, 3.5, 4.5]
    float_mask = [false, true, false, true]
    float_picked = float_data[float_mask]
    @test float_picked == [2.5, 4.5]
    @test typeof(float_picked) == Vector{Float64}
end

true
