using Test

struct InnerBox4848
    a
    b
    function InnerBox4848(x)
        new(x, "x")
    end
end

make_inner4848(flag) = InnerBox4848(flag ? 1 : 2)
use_inner4848(flag) = getfield(make_inner4848(flag), :b)

function use_inner_local4848(flag)
    box = InnerBox4848(flag ? 1 : 2)
    return getfield(box, :b)
end

@testset "PartialStruct inner constructor return field inference (Issue #4848)" begin
    @test make_inner4848(true) isa InnerBox4848
    @test use_inner4848(true) == "x"
    @test use_inner_local4848(false) == "x"

    @test Base.infer_return_type(make_inner4848, Tuple{Bool}) == InnerBox4848
    @test Base.return_types(make_inner4848, Tuple{Bool})[1] == InnerBox4848
    @test Base.infer_return_type(use_inner4848, Tuple{Bool}) == String
    @test Base.return_types(use_inner4848, Tuple{Bool})[1] == String
end

true
