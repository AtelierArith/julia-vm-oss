using Test

@testset "quoted operator function heads (Issue #7519)" begin
    ex = :(function (fcall_ | fcall_)
        body_
    end)
    sig = ex.args[1]

    @test ex isa Expr
    @test ex.head == :function
    @test sig isa Expr
    @test sig.head == :call
    @test sig.args[1] == :|
    @test sig.args[2] == :fcall_
    @test sig.args[3] == :fcall_
end

true
