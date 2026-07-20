using Test

# Issue #11306: a `finally` block whose own body catches an explicit
# `rethrow()` with a nested try/catch must NOT swallow the exception whose
# unwind entered the finally in the first place -- it must still reach the
# enclosing catch, exactly as upstream Julia does. Root cause: the pending
# "must re-propagate at the end of this finally" state used to be a single
# scalar (`rethrow_on_finally`) that any nested handler routing -- including
# the nested catch's own unrelated exception -- unconditionally clobbered.
# It is now a depth-aware stack (`Vm::pending_finally_rethrows`), truncated
# to each handler's recorded depth so a nested catch inside the finally
# cannot see, let alone clear, an enclosing finally's marker.

@testset "MWE: nested catch swallowing rethrow() inside finally (Issue #11306)" begin
    events = String[]
    outer = "not caught"
    try
        try
            error("replacement")
        finally
            try
                rethrow()
            catch e
                push!(events, "finally caught: $(e.msg)")
            end
        end
    catch e
        outer = e.msg
    end
    @test events == ["finally caught: replacement"]
    @test outer == "replacement"
end

@testset "nested catch handles then re-throws again (Issue #11306)" begin
    events = String[]
    outer = "not caught"
    try
        try
            error("boom")
        finally
            try
                rethrow()
            catch e
                push!(events, "nested caught: $(e.msg)")
                rethrow()
            end
        end
    catch e
        outer = e.msg
    end
    @test events == ["nested caught: boom"]
    @test outer == "boom"
end

@testset "double-nested finally: inner swallow still reaches outer catch (Issue #11306)" begin
    events = String[]
    outer = "not caught"
    try
        try
            try
                error("deep")
            finally
                try
                    rethrow()
                catch e
                    push!(events, "inner finally caught: $(e.msg)")
                end
            end
        finally
            push!(events, "outer finally ran")
        end
    catch e
        outer = e.msg
    end
    @test events == ["inner finally caught: deep", "outer finally ran"]
    @test outer == "deep"
end

# A finally whose nested try/catch does NOT touch rethrow() at all (an
# unrelated exception caught and fully resolved inside the finally) must
# still let the original exception through afterwards.
@testset "unrelated nested catch inside finally does not affect propagation (Issue #11306)" begin
    events = String[]
    outer = "not caught"
    try
        try
            error("original")
        finally
            try
                error("unrelated")
            catch e
                push!(events, "unrelated caught: $(e.msg)")
            end
        end
    catch e
        outer = e.msg
    end
    @test events == ["unrelated caught: unrelated"]
    @test outer == "original"
end

true
