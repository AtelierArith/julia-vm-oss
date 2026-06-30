using Test

@testset "quoted function definition pair (Issue #7517)" begin
    ex = :(begin
        function f_(args__)
            body_
        end => rhs
    end)
    pair = ex.args[2]
    lhs = pair.args[2]
    sig = lhs.args[1]

    @test ex isa Expr
    @test ex.head == :block
    @test pair isa Expr
    @test pair.head == :call
    @test pair.args[1] == Symbol("=>")
    @test lhs isa Expr
    @test lhs.head == :function
    @test sig.head == :call
    @test sig.args[1] == :f_
    @test sig.args[2] == :args__
    @test pair.args[3] == :rhs
end

true
