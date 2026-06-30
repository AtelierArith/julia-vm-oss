using Test

function same_size(A, B)
    if size(A) != size(B)
        return false
    end
    return true
end

function different_size(A, B)
    return size(A) != size(B)
end

function same_size_eq(A, B)
    return size(A) == size(B)
end

function same_size_qualified(A, B)
    return Base.size(A) == Base.size(B)
end

@testset "size tuple comparison in functions (Issue #7578)" begin
    row = [1.0 2.0]
    other_row = [3.0 4.0]
    col = [1.0; 2.0]

    @test same_size(row, other_row)
    @test same_size(row, col) == false
    @test different_size(row, col)
    @test different_size(row, other_row) == false
    @test same_size_eq(row, other_row)
    @test same_size_eq(row, col) == false
    @test same_size_qualified(row, other_row)
end

true
