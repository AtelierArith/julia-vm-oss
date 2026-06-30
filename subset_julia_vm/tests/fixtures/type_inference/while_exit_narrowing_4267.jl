using Test

function while_exit_narrowing_4267()
    x = 1
    while x isa Int64
        x = "s"
    end
    x
end

@testset "while exit narrowing preserves loop-carried assignment (Issue #4267)" begin
    @test while_exit_narrowing_4267() == "s"
    @test Base.infer_return_type(while_exit_narrowing_4267, Tuple{}) === String
    @test Base.return_types(while_exit_narrowing_4267, Tuple{})[1] === String
end

true
