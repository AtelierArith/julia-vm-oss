# Test backtrace APIs expose VM stack frames
# Issue #8993: backtrace(), catch_backtrace(), and stacktrace() must not be
# silent empty stubs.

using Test

function exceptions_backtrace_leaf_8993()
    error("bt8993")
end

function exceptions_backtrace_mid_8993()
    exceptions_backtrace_leaf_8993()
end

function exceptions_backtrace_capture_8993()
    try
        exceptions_backtrace_mid_8993()
    catch e
        bt = catch_backtrace()
        st = stacktrace(bt)
        current = stacktrace()
        first_frame = length(st) > 0 ? string(st[1]) : ""
        all_frames = join(string.(st), "\n")
        return (length(bt), length(st), length(current), first_frame, all_frames)
    end
end

function exceptions_current_backtrace_8993()
    return (length(backtrace()), length(stacktrace()))
end

@testset "Backtrace APIs expose stack frames (Issue #8993)" begin
    bt_len, st_len, current_len, first_frame, all_frames = exceptions_backtrace_capture_8993()
    @test bt_len > 0
    @test st_len > 0
    @test current_len > 0
    @test occursin("exceptions_backtrace", first_frame) ||
          occursin("exceptions_backtrace", all_frames)

    current_bt_len, current_stack_len = exceptions_current_backtrace_8993()
    @test current_bt_len > 0
    @test current_stack_len > 0

    # current_exceptions remains a minimal empty-array implementation until
    # exception-stack object parity is tackled separately.
    excs = current_exceptions()
    @test length(excs) == 0
end

true
