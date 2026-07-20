using Test

function return_channel_if_tail_8761(c::Bool)
    if c
        return 0
    end
    "s"
end

function return_channel_block_tail_8761(c::Bool)
    begin
        if c
            return 0
        end
    end
    "s"
end

function return_channel_try_tail_8761(c::Bool)
    try
        if c
            return 0
        end
    catch
        return 1
    end
    "s"
end

function return_channel_for_tail_8761(n::Int)
    for i in 1:n
        return 0
    end
    "s"
end

@testset "return channel fallthrough joins (Issue #8761)" begin
    @test Base.infer_return_type(return_channel_if_tail_8761, Tuple{Bool}) == Union{Int64, String}
    @test Base.infer_return_type(return_channel_block_tail_8761, Tuple{Bool}) == Union{Int64, String}
    @test Base.infer_return_type(return_channel_try_tail_8761, Tuple{Bool}) == Union{Int64, String}
    @test Base.infer_return_type(return_channel_for_tail_8761, Tuple{Int64}) == Union{Int64, String}

    @test return_channel_if_tail_8761(true) == 0
    @test return_channel_if_tail_8761(false) == "s"
    @test return_channel_block_tail_8761(true) == 0
    @test return_channel_block_tail_8761(false) == "s"
    @test return_channel_try_tail_8761(true) == 0
    @test return_channel_try_tail_8761(false) == "s"
    @test return_channel_for_tail_8761(1) == 0
    @test return_channel_for_tail_8761(0) == "s"
end

true
