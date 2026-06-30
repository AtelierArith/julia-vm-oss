using Test

function for_no_break_dynamic_4686(n)
    x = 1
    for i in 1:n
        x = "s"
    end
    x
end

function for_no_break_dynamic_other_4686(n)
    x = "init"
    for i in 1:n
        x = 42
    end
    x
end

function union_then_eq_4686(n)
    x = if n > 0
        "yes"
    else
        return false
    end
    x == "yes"
end

@testset "call-site == against Union-typed function return (Issue #4686)" begin
    # Inference returns Union{Int64, String}: the runtime value is the
    # String "s" so equality must NOT constant-fold to false. Before the
    # fix, the compile-time `Str-vs-non-Str` shortcut treated
    # `ValueType::Union(_)` as runtime-known and emitted `PushBool(false)`,
    # making the call-site `==` return `false` silently. The fix routes
    # `Union(_)` through the runtime-dispatch path that consults the
    # actual runtime tag.
    @test Base.return_types(for_no_break_dynamic_4686, Tuple{Int})[1] === Union{Int64, String}
    @test for_no_break_dynamic_4686(3) == "s"
    @test "s" == for_no_break_dynamic_4686(3)
    r = for_no_break_dynamic_4686(3)
    @test r == "s"

    # Reverse polarity: the Union slot holds the Int64 path at runtime —
    # equality against the String literal must be `false`, not silently
    # `true`.
    @test for_no_break_dynamic_4686(0) == 1
    @test !(for_no_break_dynamic_4686(0) == "s")

    # Int overwrite: Union{Int64, String} where the runtime value is Int.
    # The n=0 / String fall-through path hits a separate slot-allocator
    # bug on sjulia (LoadSlotI64 on a String slot) that is unrelated to
    # this issue's equality miscompile — exercise only the Int-arm here.
    @test Base.return_types(for_no_break_dynamic_other_4686, Tuple{Int})[1] === Union{Int64, String}
    @test for_no_break_dynamic_other_4686(3) == 42
    @test !(for_no_break_dynamic_other_4686(3) == "init")

    # End-to-end Union flow plus follow-up equality inside a single
    # function body — guards the through-let / through-block lowering.
    @test union_then_eq_4686(1)
    @test !union_then_eq_4686(0)
end

true
