using Test

# `while true` with no `break`, `return`, or `throw` never exits. Upstream
# Julia models the post-loop env as `Union{}` (Bottom), which propagates
# through the enclosing function's return type. (Issue #4679 follow-up to
# Issue #4267 / PR #4678.)

function while_true_no_exit_basic_4679()
    x = 1
    while true
        x = 1
    end
    x
end

function while_true_no_exit_dead_return_4679()
    while true
        x = 1
    end
    return 42
end

function while_true_no_exit_const_cond_4679()
    cond = true
    x = 1
    while cond
        x = "s"
    end
    x
end

function while_true_no_exit_nested_outer_4679()
    x = 1
    while true
        while true
            x = "inner"
            break
        end
    end
    x
end

function while_true_no_exit_literal_true_4679()
    while true
        # plain infinite loop, no body writes
    end
    "dead"
end

# Helper: a while-true with `break` still narrows to the break-exit env
# (PR #4678 path); we exercise it here together so a regression on either
# slice is caught.
function while_true_with_break_4679()
    x = 1
    while true
        x = "s"
        break
    end
    x
end

# Fail-fast checks: sjulia's @testset implementation does not propagate
# failures as a non-true file result yet, so we use plain conditions and
# fall back to runtime error to ensure the fixture surfaces regressions.
function check_no_exit_4679()
    # Issue #4679: every `while true` with no break/return/throw collapses
    # the post-loop env to Union{} in upstream Julia.
    Base.return_types(while_true_no_exit_basic_4679, Tuple{})[1] === Union{} || error("basic mismatch")
    Base.infer_return_type(while_true_no_exit_basic_4679, Tuple{}) === Union{} || error("basic infer mismatch")

    Base.return_types(while_true_no_exit_dead_return_4679, Tuple{})[1] === Union{} || error("dead_return mismatch")
    Base.infer_return_type(while_true_no_exit_dead_return_4679, Tuple{}) === Union{} || error("dead_return infer mismatch")

    Base.return_types(while_true_no_exit_const_cond_4679, Tuple{})[1] === Union{} || error("const_cond mismatch")
    Base.infer_return_type(while_true_no_exit_const_cond_4679, Tuple{}) === Union{} || error("const_cond infer mismatch")

    Base.return_types(while_true_no_exit_nested_outer_4679, Tuple{})[1] === Union{} || error("nested_outer mismatch")
    Base.infer_return_type(while_true_no_exit_nested_outer_4679, Tuple{}) === Union{} || error("nested_outer infer mismatch")

    Base.return_types(while_true_no_exit_literal_true_4679, Tuple{})[1] === Union{} || error("literal_true mismatch")
    Base.infer_return_type(while_true_no_exit_literal_true_4679, Tuple{}) === Union{} || error("literal_true infer mismatch")

    # Regression guard for PR #4678: break still produces String here.
    Base.return_types(while_true_with_break_4679, Tuple{})[1] === String || error("break regression")
    true
end

@testset "while-true with no exit collapses post-loop env to Bottom (Issue #4679)" begin
    @test Base.return_types(while_true_no_exit_basic_4679, Tuple{})[1] === Union{}
    @test Base.infer_return_type(while_true_no_exit_basic_4679, Tuple{}) === Union{}

    @test Base.return_types(while_true_no_exit_dead_return_4679, Tuple{})[1] === Union{}
    @test Base.infer_return_type(while_true_no_exit_dead_return_4679, Tuple{}) === Union{}

    @test Base.return_types(while_true_no_exit_const_cond_4679, Tuple{})[1] === Union{}
    @test Base.infer_return_type(while_true_no_exit_const_cond_4679, Tuple{}) === Union{}

    @test Base.return_types(while_true_no_exit_nested_outer_4679, Tuple{})[1] === Union{}
    @test Base.infer_return_type(while_true_no_exit_nested_outer_4679, Tuple{}) === Union{}

    @test Base.return_types(while_true_no_exit_literal_true_4679, Tuple{})[1] === Union{}
    @test Base.infer_return_type(while_true_no_exit_literal_true_4679, Tuple{}) === Union{}

    @test Base.return_types(while_true_with_break_4679, Tuple{})[1] === String
end

check_no_exit_4679()
