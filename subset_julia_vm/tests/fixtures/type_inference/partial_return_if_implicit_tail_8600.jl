using Test

function partial_if_tail_8600(c::Bool)
    if c
        return 0
    end
    "s"
end

function partial_if_else_tail_8600(c::Bool)
    if c
        return 0
    else
        c
    end
    "s"
end

function both_return_if_8600(c::Bool)
    if c
        return 1
    else
        return 2
    end
    "unreachable"
end

@testset "partial-return if joins implicit tail (Issue #8600)" begin
    @test Base.infer_return_type(partial_if_tail_8600, Tuple{Bool}) == Union{Int64, String}
    @test Base.infer_return_type(partial_if_else_tail_8600, Tuple{Bool}) == Union{Int64, String}
    @test partial_if_tail_8600(true) == 0
    @test partial_if_tail_8600(false) == "s"
    @test partial_if_else_tail_8600(true) == 0
    @test partial_if_else_tail_8600(false) == "s"

    @test Base.infer_return_type(both_return_if_8600, Tuple{Bool}) === Int64
    @test both_return_if_8600(true) == 1
    @test both_return_if_8600(false) == 2
end

true
