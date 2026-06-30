using Test

function while_true_break_break_only_4267()
    x = 1
    while true
        x = "s"
        break
    end
    x
end

function while_true_break_const_cond_4267()
    x = 1
    cond = true
    while cond
        x = "s"
        break
    end
    x
end

function while_true_break_in_if_4267()
    x = 1
    while true
        if x > 100
            x = "big"
            break
        end
        x = x + 1
    end
    x
end

function while_true_break_overwrite_4267()
    x = "init"
    while true
        x = 42
        break
    end
    x
end

function while_true_nested_break_4267()
    x = 1
    while true
        while true
            x = "inner"
            break
        end
        x = 2.0
        break
    end
    x
end

@testset "while true + break exit narrows post-loop env (Issue #4267)" begin
    @test while_true_break_break_only_4267() == "s"
    @test Base.return_types(while_true_break_break_only_4267, Tuple{})[1] === String
    @test Base.infer_return_type(while_true_break_break_only_4267, Tuple{}) === String

    @test while_true_break_const_cond_4267() == "s"
    @test Base.return_types(while_true_break_const_cond_4267, Tuple{})[1] === String

    @test while_true_break_in_if_4267() == "big"
    @test Base.return_types(while_true_break_in_if_4267, Tuple{})[1] === String

    @test while_true_break_overwrite_4267() == 42
    @test Base.return_types(while_true_break_overwrite_4267, Tuple{})[1] === Int64

    @test while_true_nested_break_4267() == 2.0
    @test Base.return_types(while_true_nested_break_4267, Tuple{})[1] === Float64
end

true
