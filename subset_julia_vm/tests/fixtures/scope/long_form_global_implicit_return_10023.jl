# Long-form function bodies whose last statement is a `global` assignment
# must return the assigned value, matching upstream Julia (an assignment
# expression evaluates to the assigned value; this includes `global x = v`).
#
# Distinct from Issue #9817 (short-form `f() = global x = 1` bodies failing to
# lower `global` at all): here the module binding was already updated
# correctly, but the function's implicit return value was `nothing` instead
# of the assigned value.

using Test

function set_long_global_10023()
    global discovered_long_global_10023 = 44
end

function set_long_global_float_10023()
    global discovered_long_global_float_10023 = 3.14
end

function set_long_global_string_10023()
    global discovered_long_global_string_10023 = "hello"
end

function set_long_global_tuple_10023()
    global discovered_long_global_tuple_10023 = (1, 2, 3)
end

# Explicit `return` after a `global` assignment must still take precedence.
function set_long_global_explicit_return_10023()
    global explicit_long_global_10023 = 10
    return explicit_long_global_10023 + 1
end

# A normal (non-`global`) assignment in tail position is unaffected (Issue #8976).
function set_long_local_assign_tail_10023()
    local y = 7
    y = y + 1
    y
end

# `global` assignment nested inside an `if`'s tail branches also returns the
# assigned value, not `nothing` (same lowering + inference path).
function set_long_global_in_if_10023(flag::Bool)
    if flag
        global branched_long_global_10023 = 100
    else
        global branched_long_global_10023 = 200
    end
end

@testset "long-form global assignment implicit return (Issue #10023)" begin
    @test set_long_global_10023() == 44
    @test discovered_long_global_10023 == 44

    @test set_long_global_float_10023() == 3.14
    @test discovered_long_global_float_10023 == 3.14

    @test set_long_global_string_10023() == "hello"
    @test discovered_long_global_string_10023 == "hello"

    @test set_long_global_tuple_10023() == (1, 2, 3)
    @test discovered_long_global_tuple_10023 == (1, 2, 3)

    @test set_long_global_explicit_return_10023() == 11
    @test explicit_long_global_10023 == 10

    @test set_long_local_assign_tail_10023() == 8

    @test set_long_global_in_if_10023(true) == 100
    @test branched_long_global_10023 == 100
    @test set_long_global_in_if_10023(false) == 200
    @test branched_long_global_10023 == 200
end

true
