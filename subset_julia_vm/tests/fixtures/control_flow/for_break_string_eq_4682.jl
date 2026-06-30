using Test

function while_v_4682()
    x = 1
    while true
        x = "s"
        break
    end
    x
end

function for_v_4682()
    x = 1
    for i in 1:3
        x = "s"
        break
    end
    x
end

function for_v_int_overwrite_4682()
    x = "init"
    for i in 1:3
        x = 42
        break
    end
    x
end

@testset "for-loop with break: call-site equality (Issue #4682)" begin
    # Direct-call equality at the call site must match the via-binding
    # path. Before the Issue #4680 narrowing landed, the for-loop
    # variant returned `Union{Int64, String}` from inference and the
    # call-site `==` picked a generic-dispatch path that returned
    # `false` even though the runtime value was the `String "s"`. The
    # while-loop variant already narrowed via PR #4678 so it worked.
    @test while_v_4682() == "s"
    @test for_v_4682() == "s"
    @test for_v_4682() === "s"

    # Via-binding parity: the binding-then-compare path was already
    # correct on `main`. Keep it as a guardrail so a future narrowing
    # regression that re-widens `for_v` cannot silently revert this
    # pattern.
    r = for_v_4682()
    @test r == "s"

    # Reverse direction (literal == call) must agree too.
    @test "s" == for_v_4682()

    # Numeric overwrite variant: catches an over-eager narrowing that
    # would assume the post-loop variable is always `String`.
    @test for_v_int_overwrite_4682() == 42
    @test 42 == for_v_int_overwrite_4682()
end

true
