using Test

# Issue #11281: try/catch/else/finally are independent hard scopes. A name
# introduced by one clause is visible to closures created in that clause, but
# not to sibling clauses or code after the try statement. Existing enclosing
# locals and explicit globals remain ordinary updates.

function try_local_does_not_escape_11281()
    try
        try_only_11281 = 1
    catch
    end
    return @isdefined try_only_11281
end

function catch_locals_do_not_escape_11281()
    try
        error("catch path")
    catch catch_binder_11281
        catch_only_11281 = 2
    end
    binder_defined = @isdefined catch_binder_11281
    local_defined = @isdefined catch_only_11281
    return (binder_defined, local_defined)
end

function else_local_does_not_escape_11281()
    try
        1
    catch
    else
        else_only_11281 = 3
    end
    return @isdefined else_only_11281
end

function finally_local_does_not_escape_11281()
    try
        1
    finally
        finally_only_11281 = 4
    end
    return @isdefined finally_only_11281
end

# Leading underscores are ordinary Julia identifier characters. They must not
# be mistaken for sjulia-generated temporaries when deciding clause ownership.
function compiler_like_user_name_does_not_escape_11281()
    try
        __sj_user_11281 = 5
    catch
    end
    return @isdefined __sj_user_11281
end

function explicit_local_assignment_shadows_11281()
    x = 6
    try
        local x = 7
        @test x == 7
    catch
    end
    return x
end

function bare_local_then_assignment_shadows_11281()
    x = 8
    try
        local x
        x = 9
        @test x == 9
    catch
    end
    return x
end

function assignment_expression_does_not_escape_11281()
    try
        1 + (assign_expr_local_11281 = 10)
    catch
    end
    return @isdefined assign_expr_local_11281
end

macro explicit_local_assignment_11281()
    esc(:(local macro_local_11281 = 20))
end

macro capture_like_assignment_11281()
    esc(quote
        capture_like_target_11281 = nothing
        capture_like_target_11281 = 21
        true
    end)
end

function macro_explicit_local_shadows_clause_only_11281()
    macro_local_11281 = 10
    try
        @explicit_local_assignment_11281
    catch
    end
    return macro_local_11281
end


function fresh_macro_capture_does_not_escape_11281()
    @test @capture_like_assignment_11281
    return @isdefined capture_like_target_11281
end

@testset "fresh clause locals do not escape (Issue #11281)" begin
    @test !try_local_does_not_escape_11281()
    @test catch_locals_do_not_escape_11281() == (false, false)
    @test !else_local_does_not_escape_11281()
    @test !finally_local_does_not_escape_11281()
    @test !compiler_like_user_name_does_not_escape_11281()
    @test explicit_local_assignment_shadows_11281() == 6
    @test bare_local_then_assignment_shadows_11281() == 8
    @test !assignment_expression_does_not_escape_11281()
    @test macro_explicit_local_shadows_clause_only_11281() == 10
    @test !fresh_macro_capture_does_not_escape_11281()
end

function catch_cannot_see_try_local_11281()
    try
        try_sibling_11281 = 10
        error("leave try")
    catch
        return @isdefined try_sibling_11281
    end
end

function named_function_live_caller_11281()
    caller = () -> -1
    try
        function clause_named_function_11281()
            71
        end
        caller = () -> clause_named_function_11281()
    catch
    end
    named_defined = @isdefined clause_named_function_11281
    return (named_defined, caller)
end

@testset "named clause functions remain live only through captures (Issue #11281)" begin
    named_defined, caller = named_function_live_caller_11281()
    @test !named_defined
    @test caller() == 71
end

function else_cannot_see_try_local_11281()
    try
        try
            try_else_sibling_11281 = 11
        catch
        else
            return @isdefined try_else_sibling_11281
        end
    catch err
        return err
    end
end

function finally_cannot_see_catch_local_11281()
    seen = false
    try
        error("enter catch")
    catch
        catch_finally_sibling_11281 = 12
    finally
        seen = @isdefined catch_finally_sibling_11281
    end
    return seen
end

@testset "sibling clauses are isolated (Issue #11281)" begin
    @test !catch_cannot_see_try_local_11281()
    @test !else_cannot_see_try_local_11281()
    @test !finally_cannot_see_catch_local_11281()
end

function same_clause_captures_11281()
    try_capture = () -> 0
    catch_capture = () -> 0
    else_capture = () -> 0
    finally_capture = () -> 0
    try
        try_captured_11281 = 21
        try_capture = () -> try_captured_11281
    catch
    else
        else_captured_11281 = 22
        else_capture = () -> else_captured_11281
    finally
        finally_captured_11281 = 23
        finally_capture = () -> finally_captured_11281
    end
    try
        error("capture catch")
    catch catch_captured_11281
        catch_local_captured_11281 = 24
        catch_capture = () -> (catch_captured_11281, catch_local_captured_11281)
    end
    return (try_capture, catch_capture, else_capture, finally_capture)
end

@testset "same-clause captures survive scope exit (Issue #11281)" begin
    try_capture, catch_capture, else_capture, finally_capture = same_clause_captures_11281()
    @test try_capture() == 21
    caught, catch_local = catch_capture()
    @test caught == ErrorException("capture catch")
    @test catch_local == 24
    @test else_capture() == 22
    @test finally_capture() == 23
end

function enclosing_updates_11281()
    try_value = 0
    catch_value = 0
    else_value = 0
    finally_value = 0
    binder = "outer"
    try
        try_value = 31
    catch
    else
        else_value = 32
    finally
        finally_value = 33
    end
    try
        error("replace binder")
    catch binder
        catch_value = 34
    end
    return (try_value, catch_value, else_value, finally_value, binder)
end

function later_enclosing_assignment_11281()
    try
        later_outer_11281 = 40
    catch
    end
    later_outer_11281 = 41
    return later_outer_11281
end

@testset "enclosing locals still update (Issues #11281/#10999)" begin
    try_value, catch_value, else_value, finally_value, binder = enclosing_updates_11281()
    @test (try_value, catch_value, else_value, finally_value) == (31, 34, 32, 33)
    @test binder == ErrorException("replace binder")
    @test later_enclosing_assignment_11281() == 41
end

global_try_11281 = 0
global_catch_11281 = 0
global_else_11281 = 0
global_finally_11281 = 0

function explicit_global_updates_11281()
    try
        global global_try_11281 = 51
    catch
    else
        global global_else_11281 = 52
    finally
        global global_finally_11281 = 53
    end
    try
        error("global catch")
    catch
        global global_catch_11281 = 54
    end
end

explicit_global_updates_11281()
@testset "explicit globals survive clause exit (Issue #11281)" begin
    @test global_try_11281 == 51
    @test global_catch_11281 == 54
    @test global_else_11281 == 52
    @test global_finally_11281 == 53
end

module ClauseModuleGlobal11281
module_value_11281 = 0
try
    error("module catch")
catch
    global module_value_11281 = 55
end

loop_value_11281 = 0
for i in 1:10
    global loop_value_11281 += i
end
end

@testset "module clause globals retain their owner (Issue #11281)" begin
    @test ClauseModuleGlobal11281.module_value_11281 == 55
    @test ClauseModuleGlobal11281.loop_value_11281 == 55
end

function exceptional_inner_try_cleanup_11281()
    try
        try
            exceptional_try_local_11281 = 61
            error("inner try")
        finally
            exceptional_finally_local_11281 = 62
        end
    catch
        try_defined = @isdefined exceptional_try_local_11281
        finally_defined = @isdefined exceptional_finally_local_11281
        return (try_defined, finally_defined)
    end
end

function exceptional_inner_catch_cleanup_11281()
    try
        try
            error("inner catch entry")
        catch exceptional_binder_11281
            exceptional_catch_local_11281 = 63
            error("leave inner catch")
        end
    catch
        binder_defined = @isdefined exceptional_binder_11281
        local_defined = @isdefined exceptional_catch_local_11281
        return (binder_defined, local_defined)
    end
end

function exceptional_else_cleanup_11281()
    try
        try
            1
        catch
        else
            exceptional_else_local_11281 = 66
            error("leave else")
        end
    catch
        return @isdefined exceptional_else_local_11281
    end
end

function exceptional_finally_cleanup_11281()
    try
        try
            1
        finally
            exceptional_finally_only_11281 = 67
            error("leave finally")
        end
    catch
        return @isdefined exceptional_finally_only_11281
    end
end

function catch_else_structured_exit_cleanup_11281()
    for i in 1:2
        try
            error("catch continue")
        catch
            catch_continue_local_11281 = i
            continue
        end
    end
    for i in 1:2
        try
            1
        catch
        else
            else_break_local_11281 = i
            break
        end
    end
    return (
        @isdefined(catch_continue_local_11281),
        @isdefined(else_break_local_11281),
    )
end

function return_from_catch_capture_11281()
    try
        error("catch return")
    catch
        catch_return_local_11281 = 68
        return () -> catch_return_local_11281
    end
end

function return_from_else_capture_11281()
    try
        1
    catch
    else
        else_return_local_11281 = 69
        return () -> else_return_local_11281
    end
end

function catch_throw_cleanup_preserves_rethrow_state_11281()
    outer_message = ""
    reraised_message = ""
    finally_ran = false
    catch_local_defined = true
    try
        try
            error("original catch state")
        catch
            catch_throw_local_11281 = 70
            error("replacement catch state")
        finally
            finally_ran = true
        end
    catch outer
        outer_message = outer.msg
        catch_local_defined = @isdefined catch_throw_local_11281
        try
            rethrow()
        catch reraised
            reraised_message = reraised.msg
        end
    end

    outside_message = ""
    try
        rethrow()
    catch outside
        outside_message = outside.msg
    end
    return (
        catch_local_defined,
        finally_ran,
        outer_message,
        reraised_message,
        outside_message,
    )
end

global_shadow_explicit_11281 = 10
function explicit_clause_local_overrides_outer_global_11281()
    global global_shadow_explicit_11281
    try
        local global_shadow_explicit_11281 = 20
    catch
    end
    return global_shadow_explicit_11281
end

global_clause_boundary_11281 = 10
function clause_global_does_not_reclassify_following_assignment_11281()
    try
        global global_clause_boundary_11281 = 20
    catch
    end
    global_clause_boundary_11281 = 30
    return global_clause_boundary_11281
end

loop_for_global_11281 = 10
function numeric_for_global_stays_in_loop_11281()
    try
        for i in 1:1
            global loop_for_global_11281 = 20
        end
        loop_for_global_11281 = 30
        return loop_for_global_11281
    catch e
        return e
    end
end

direct_loop_global_11281 = 10
function direct_loop_global_stays_in_loop_11281()
    for i in 1:1
        global direct_loop_global_11281 = 20
    end
    direct_loop_global_11281 = 30
    return direct_loop_global_11281
end

loop_foreach_global_11281 = 10
function foreach_global_stays_in_loop_11281()
    try
        for i in [1]
            global loop_foreach_global_11281 = 20
        end
        loop_foreach_global_11281 = 30
        return loop_foreach_global_11281
    catch e
        return e
    end
end

loop_tuple_global_11281 = 10
function tuple_foreach_global_stays_in_loop_11281()
    try
        for (i, j) in [(1, 2)]
            global loop_tuple_global_11281 = 20
        end
        loop_tuple_global_11281 = 30
        return loop_tuple_global_11281
    catch e
        return e
    end
end

loop_while_global_11281 = 10
function while_global_stays_in_loop_11281()
    ran = false
    try
        while !ran
            ran = true
            global loop_while_global_11281 = 20
        end
        loop_while_global_11281 = 30
        return loop_while_global_11281
    catch e
        return e
    end
end

transparent_if_global_11281 = 10
function transparent_if_global_stays_in_clause_11281()
    try
        if true
            global transparent_if_global_11281 = 20
        end
        transparent_if_global_11281 = 30
        return transparent_if_global_11281
    catch e
        return e
    end
end

@testset "global provenance respects clause boundaries (Issue #11281)" begin
    @test explicit_clause_local_overrides_outer_global_11281() == 10
    @test global_shadow_explicit_11281 == 10
    @test clause_global_does_not_reclassify_following_assignment_11281() == 30
    @test global_clause_boundary_11281 == 20
    @test (numeric_for_global_stays_in_loop_11281(), loop_for_global_11281) == (30, 20)
    @test (direct_loop_global_stays_in_loop_11281(), direct_loop_global_11281) == (30, 20)
    @test (foreach_global_stays_in_loop_11281(), loop_foreach_global_11281) == (30, 20)
    @test (tuple_foreach_global_stays_in_loop_11281(), loop_tuple_global_11281) == (30, 20)
    @test (while_global_stays_in_loop_11281(), loop_while_global_11281) == (30, 20)
    @test (transparent_if_global_stays_in_clause_11281(), transparent_if_global_11281) ==
          (30, 30)
end

function break_continue_cleanup_11281()
    for i in 1:2
        try
            break_local_11281 = i
            break
        catch
        end
    end
    for i in 1:2
        try
            continue_local_11281 = i
            continue
        catch
        end
    end
    break_defined = @isdefined break_local_11281
    continue_defined = @isdefined continue_local_11281
    return (break_defined, continue_defined)
end

function return_capture_cleanup_11281()
    try
        return_local_11281 = 64
        return () -> return_local_11281
    finally
        return_finally_local_11281 = 65
    end
end

@testset "structured clause exits isolate fresh locals (Issue #11281)" begin
    @test exceptional_inner_try_cleanup_11281() == (false, false)
    @test exceptional_inner_catch_cleanup_11281() == (false, false)
    @test break_continue_cleanup_11281() == (false, false)
    @test return_capture_cleanup_11281()() == 64
    @test !exceptional_else_cleanup_11281()
    @test !exceptional_finally_cleanup_11281()
    @test catch_else_structured_exit_cleanup_11281() == (false, false)
    @test return_from_catch_capture_11281()() == 68
    @test return_from_else_capture_11281()() == 69
    @test catch_throw_cleanup_preserves_rethrow_state_11281() == (
        false,
        true,
        "replacement catch state",
        "replacement catch state",
        "rethrow() not allowed outside a catch block",
    )
end

true
