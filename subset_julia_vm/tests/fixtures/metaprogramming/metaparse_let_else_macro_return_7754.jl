using Test

# Issue #7754: Meta.parse must normalize parser-internal let/else nodes before
# eval and macro-return lowering see the Expr value.

macro emit7754(src)
    return Meta.parse(src)
end

@testset "Meta.parse let/else eval and macro-return (Issue #7754)" begin
    let_ex = Meta.parse("let x = 2; x + 3 end")
    @test let_ex.head == :let
    @test let_ex.args[1].head == :(=)

    if_ex = Meta.parse("if true; 10; else; 20; end")
    @test if_ex.head == :if
    @test if_ex.args[2].head == :block
    @test if_ex.args[3].head == :block

    elseif_ex = Meta.parse("if false; 10; elseif true; 20; else; 30; end")
    @test elseif_ex.args[3].head == :elseif

    @test eval(let_ex) == 5
    @test eval(if_ex) == 10
    @test eval(elseif_ex) == 20

    @test (@emit7754 "let x = 2; x + 3 end") == 5
    @test (@emit7754 "if true; 10; else; 20; end") == 10
    @test (@emit7754 "if false; 10; elseif true; 20; else; 30; end") == 20
end

eval(Meta.parse("let x = 2; x + 3 end")) == 5 &&
    eval(Meta.parse("if true; 10; else; 20; end")) == 10 &&
    eval(Meta.parse("if false; 10; elseif true; 20; else; 30; end")) == 20 &&
    (@emit7754 "let x = 2; x + 3 end") == 5 &&
    (@emit7754 "if true; 10; else; 20; end") == 10 &&
    (@emit7754 "if false; 10; elseif true; 20; else; 30; end") == 20
