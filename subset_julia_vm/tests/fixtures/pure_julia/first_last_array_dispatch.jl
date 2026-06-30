# Test first/last on Array still works through Pure Julia generic dispatch (Issue #3734).
# Generic first(arr) / last(arr) is implemented in base/range.jl using
# arr[1] / arr[length(arr)].

using Test

@testset "first/last on Array works via Pure Julia (Issue #3734)" begin
    a = [11, 22, 33]
    @test (first(a) == 11)
    @test (last(a) == 33)
end

true
