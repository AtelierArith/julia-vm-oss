# Line-leading operator-function call in statement position (Issue #5337)
#
# A statement that begins with `+(...)` / `-(...)` / `*(...)` (a unary-capable or
# plain operator followed by `(`) was misparsed as the start of an operator
# method definition and required a trailing `=`, producing ParseFailed. It must
# parse as an ordinary prefix operator-function call statement and run.

using Test

function s_splat(t)
    +(t...)
end

function s_add()
    +(1, 2)
end

function p_mul(t)
    *(t...)
end

g_sub(t) = -(t...)

@testset "line-leading operator call as a statement (Issue #5337)" begin
    @test s_splat((10, 20, 30)) == 60
    @test s_add() == 3
    @test p_mul((2, 3, 4)) == 24
    @test g_sub((10, 3)) == 7

    # An actual operator method definition still parses (trailing `=`).
    @test (+(1, 2)) == 3
end

true
