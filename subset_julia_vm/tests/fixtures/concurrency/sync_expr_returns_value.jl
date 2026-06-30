# Regression test for Issue #7813:
# Expression-position `@sync` (RHS of an assignment / explicit return value) must
# evaluate to the value of its body's last expression, mirroring upstream Julia
# (`let v = body; sync_end(...); v end`). Previously the lowered `Expr::LetBlock`
# ended on the value-less `if !isempty(exceptions); throw(...); end` guard, so the
# block yielded `nothing` instead of the body value (e.g. a `Task`).

using Test

@testset "@sync block expr yields last @async Task (Issue #7813)" begin
    function f()
        r = @sync begin
            @async 1
            @async 2
        end
        return r
    end
    r = f()
    @test isa(r, Task)
    @test fetch(r) == 2
end

@testset "@sync single @async expr yields the Task (Issue #7813)" begin
    function g()
        s = @sync @async 99
        return s
    end
    s = g()
    @test isa(s, Task)
    @test fetch(s) == 99
end

@testset "@sync block expr yields trailing assigned @async Task (Issue #7813)" begin
    function h()
        r = @sync begin
            @async 1
            t = @async 42
        end
        return r
    end
    r = h()
    @test isa(r, Task)
    @test fetch(r) == 42
end

@testset "@sync block expr yields trailing plain value (Issue #7813)" begin
    function k()
        r = @sync begin
            @async 7
            123
        end
        return r
    end
    @test k() == 123
    @test k() isa Int
end

# Exception aggregation must still surface a CompositeException even though the
# block now yields a value when it succeeds.
@testset "@sync expr still aggregates standalone @async failures (Issue #7813)" begin
    ex = nothing
    try
        r = @sync begin
            @async error("first")
            @async error("second")
        end
    catch e
        ex = e
    end
    @test isa(ex, CompositeException)
    @test length(ex) == 2
end

@testset "@sync single @async expr still aggregates failure (Issue #7813)" begin
    ex = nothing
    try
        s = @sync @async error("single")
    catch e
        ex = e
    end
    @test isa(ex, CompositeException)
    @test length(ex) == 1
end

true
