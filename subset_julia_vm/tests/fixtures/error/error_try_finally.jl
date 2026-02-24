using Test

# Tests for try/catch/finally semantics.
# finally block must always execute regardless of whether an exception was thrown.

function finally_runs_on_success()
    log = String[]
    try
        push!(log, "try")
    finally
        push!(log, "finally")
    end
    return log
end

function finally_runs_on_exception()
    log = String[]
    try
        push!(log, "try")
        throw(ErrorException("test"))
        push!(log, "unreachable")
    catch e
        push!(log, "catch")
    finally
        push!(log, "finally")
    end
    return log
end

function finally_runs_even_without_catch()
    log = String[]
    caught = false
    try
        try
            push!(log, "inner try")
            throw(ErrorException("test"))
        finally
            push!(log, "inner finally")
        end
    catch e
        caught = true
        push!(log, "outer catch")
    end
    return (log, caught)
end

function finally_return_value()
    x = 0
    try
        x = 1
        throw(ErrorException("test"))
    catch e
        x = 2
    finally
        x += 10
    end
    return x
end

function try_without_exception()
    x = 0
    try
        x = 5
    catch e
        x = -1
    finally
        x += 100
    end
    return x
end

@testset "try/finally basic semantics" begin
    @test finally_runs_on_success() == ["try", "finally"]
    @test finally_runs_on_exception() == ["try", "catch", "finally"]
    @test finally_return_value() == 12   # catch sets x=2, finally adds 10
    @test try_without_exception() == 105 # try sets x=5, finally adds 100
end

@testset "finally runs without catch block" begin
    (log, caught) = finally_runs_even_without_catch()
    @test log == ["inner try", "inner finally", "outer catch"]
    @test caught == true
end

true
