using Test

@testset "Sys.WORD_SIZE module binding (Issue #6096)" begin
    @test Sys.WORD_SIZE == 32 || Sys.WORD_SIZE == 64
    @test typeof(Sys.WORD_SIZE) === Int
    @test isdefined(Sys, :WORD_SIZE)
    @test getfield(Sys, :WORD_SIZE) == Sys.WORD_SIZE
end

true
