# Regression test for Issue #7768:
# Statement-position `@sync begin ... end` must execute its body in the enclosing
# scope so that an assignment such as `t = @async ...` updates the surrounding
# local instead of writing to an isolated let-block local. Previously the
# statement path routed through the expression `Expr::LetBlock` lowering, so the
# inner assignment never reached the outer `t` and `istaskdone(t)` raised
# `MethodError: no method matching istaskdone(::Nothing)`.

using Test

@testset "@sync stmt preserves outer-local assignment (Issue #7768)" begin
    t = nothing
    @sync begin
        t = @async 21
    end
    @test isa(t, Task)
    @test istaskdone(t)
    @test fetch(t) == 21
end

@testset "@sync stmt mixes assignment, side effect, and outer locals" begin
    t = nothing
    x = 0
    @sync begin
        x = 5
        t = @async 2 + 3
    end
    @test x == 5
    @test isa(t, Task)
    @test fetch(t) == 5
end

@testset "@sync stmt multiple assigned async update outer locals" begin
    a = nothing
    b = nothing
    @sync begin
        a = @async 10
        b = @async 20
    end
    @test isa(a, Task)
    @test isa(b, Task)
    @test fetch(a) == 10
    @test fetch(b) == 20
end

# Exception aggregation must still work in statement position: an assigned
# `@async` failure surfaces as a CompositeException while the task is still
# bound to the outer local.
@testset "@sync stmt aggregates assigned @async failure" begin
    t = nothing
    ex = nothing
    try
        @sync begin
            t = @async error("assigned")
        end
    catch e
        ex = e
    end
    @test isa(t, Task)
    @test istaskfailed(t)
    @test isa(ex, CompositeException)
    @test length(ex) == 1
end

true
