# Named functions defined in a testset capture that hard scope's locals.

using Test

@testset "testset-local named function capture (Issue #11260)" begin
    events = Int[]
    lhs_11260() = (push!(events, 1); 2)

    @test lhs_11260() == 2
    @test events == [1]

    offset = 40
    function add_offset_11260(x)
        x + offset
    end

    @test add_offset_11260(2) == 42
end

let anchor = 0
    if true
        branch_events_11260 = Int[]
    end
    branch_push_11260() = (push!(branch_events_11260, 1); length(branch_events_11260))

    @test branch_push_11260() == 1
    @test branch_events_11260 == [1]
end

let anchor = 0
    try
        error("catch capture 11260")
    catch err_11260
        catch_identity_11260() = err_11260
        @test catch_identity_11260() === err_11260
    end
end

# Every try clause is a hard scope. New clause locals must not be captured by a
# later function, and an else clause must not inherit a try-clause local. Call
# the global probes after the enclosing let exits so this specifically tests
# capture pre-analysis; same-frame dynamic visibility is tracked by Issue #11281.
let anchor = 0
    try
        try_local_11260 = 1
    catch
        try_local_11260 = 2
    end
    global function after_try_11260()
        try_local_11260
    end

    try
        try_only_11260 = 1
    catch
    else
        global function else_reads_try_11260()
            try_only_11260
        end
    end

    try
        error("catch scope 11260")
    catch catch_error_11260
        catch_local_11260 = 2
    end
    global function after_catch_local_11260()
        catch_local_11260
    end

    try
        nothing
    finally
        finally_local_11260 = 3
    end
    global function after_finally_11260()
        finally_local_11260
    end
end

@test_throws UndefVarError after_try_11260()
@test_throws UndefVarError else_reads_try_11260()
@test_throws UndefVarError after_catch_local_11260()
@test_throws UndefVarError after_finally_11260()

# Issue #11249 negative invariant: a later local assignment must not be offered
# as a capture to a function created before that assignment.
let anchor = 0
    before_later_11260() = later_11260
    later_11260 = 41
    @test later_11260 == 41
end

true
