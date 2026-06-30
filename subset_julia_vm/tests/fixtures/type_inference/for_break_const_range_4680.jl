using Test

function for_break_const_range_break_only_4680()
    x = 1
    for i in 1:3
        x = "s"
        break
    end
    x
end

function for_break_const_range_overwrite_4680()
    x = "init"
    for i in 1:3
        x = 42
        break
    end
    x
end

function for_break_const_range_with_step_4680()
    x = 1
    for i in 1:2:6
        x = "s"
        break
    end
    x
end

function for_break_const_range_negative_step_4680()
    x = 1
    for i in 5:-1:1
        x = "s"
        break
    end
    x
end

function for_break_dynamic_range_4680(n)
    x = 1
    for i in 1:n
        x = "s"
        break
    end
    x
end

function for_nested_break_const_range_4680()
    x = 1
    for i in 1:3
        for j in 1:2
            x = "inner"
            break
        end
        x = 2.0
        break
    end
    x
end

@testset "for + break over non-empty constant range narrows post-loop env (Issue #4680)" begin
    # Runtime uses `===` rather than `==` because the for-loop call-site
    # equality path on sjulia currently reports `false` even when the
    # function returns the same literal (tracked separately as Issue #4682).
    # Identity (`===`) compares interned literals safely and lets the
    # inference assertions below remain the focus of this fixture.
    @test for_break_const_range_break_only_4680() === "s"
    @test Base.return_types(for_break_const_range_break_only_4680, Tuple{})[1] === String
    @test Base.infer_return_type(for_break_const_range_break_only_4680, Tuple{}) === String

    @test for_break_const_range_overwrite_4680() === 42
    @test Base.return_types(for_break_const_range_overwrite_4680, Tuple{})[1] === Int64

    @test for_break_const_range_with_step_4680() === "s"
    @test Base.return_types(for_break_const_range_with_step_4680, Tuple{})[1] === String

    @test for_break_const_range_negative_step_4680() === "s"
    @test Base.return_types(for_break_const_range_negative_step_4680, Tuple{})[1] === String

    # Dynamic bound: zero iterations is possible, so the pre-loop env
    # still falls through. Both sjulia and upstream Julia keep the wider
    # Union — this contrast pins the precision difference vs the
    # non-empty cases above. Runtime calls were omitted in the initial
    # slice because `1:n` + `break` hung sjulia; PR #4689 (Issue #4684)
    # fixed the specializer's break/continue patching, so the runtime
    # assertions are now safe and restored here. (Issue #4690)
    @test for_break_dynamic_range_4680(3) === "s"
    @test for_break_dynamic_range_4680(0) === 1
    @test Base.return_types(for_break_dynamic_range_4680, Tuple{Int})[1] === Union{Int64, String}

    @test for_nested_break_const_range_4680() === 2.0
    @test Base.return_types(for_nested_break_const_range_4680, Tuple{})[1] === Float64
end

true
