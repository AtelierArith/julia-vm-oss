using Test

# Issue #7755: Meta.parse keyword calls use Expr(:kw, ...). Runtime eval and
# macro-return lowering must both restore that shape to a keyword call.

kw7755(; a=0, b=0) = a + b

macro emit7755(src)
    return Meta.parse(src)
end

@testset "Meta.parse keyword calls eval and macro-return (Issue #7755)" begin
    @test eval(Meta.parse("kw7755(a=2, b=3)")) == 5
    @test (@emit7755 "kw7755(a=2, b=3)") == 5
end

eval(Meta.parse("kw7755(a=2, b=3)")) == 5 &&
    (@emit7755 "kw7755(a=2, b=3)") == 5
