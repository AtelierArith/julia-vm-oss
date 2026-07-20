using Test

# Issue #9358: a `try`/`catch` (or `if`) used as the EXPRESSION VALUE of a block
# must yield the branch value, not `nothing`. Root cause: a `begin ... end` block
# in value position (and the `x = begin ... end` assignment fast path) dropped the
# value of a trailing control-flow statement (`Stmt::If` / `Stmt::Try`), running it
# for effect and defaulting to `nothing`. The headline symptom was a generator /
# comprehension body wrapped in `begin ... try ... catch ... end end` collecting
# `nothing` for every element while the per-element side effects fired correctly.
#
# Upstream Julia: "the last statement of a block is its value". Verified at parity
# with upstream `julia` (side-effect order, error timing, and the exact values).

@testset "Issue #9358 generator/comprehension body with try/catch value" begin
    # The exact MWE from #9358: a begin-block whose tail is a try/catch expression,
    # used as a generator body. The catch handles 10 ÷ 0; the mapped value is the
    # value of the try/catch, not `nothing`.
    function scenarioC()
        log = String[]
        r = collect(begin
            try
                push!(log, "b$x")
                10 ÷ x
            catch
                -1
            end
        end for x in [1, 0, 2])
        return (r, log)
    end
    r, log = scenarioC()
    @test r == [10, -1, 5]
    @test log == ["b1", "b0", "b2"]   # body runs once per element, in order

    # Bracket comprehension form (same body). `==` is eltype-agnostic so this
    # passes whether the inferred eltype is Int64 or Any.
    c1 = [begin
        try
            100 ÷ x
        catch
            -1
        end
    end for x in [2, 0, 5]]
    @test c1 == [50, -1, 20]

    # An `if` (not `try`) tail in a begin-block generator body — eltype is Int64.
    g1 = collect(begin
        if x > 0
            x * 2
        else
            -x
        end
    end for x in [-1, 2, -3])
    @test g1 == [1, 4, 3]
end

@testset "begin-block as value: control-flow tail (assignment / top level)" begin
    # `x = begin ...; if/elseif/else; end` — the assignment fast path must assign
    # the branch value.
    k = begin
        s = "x"
        if s == "a"
            1
        elseif s == "x"
            2
        else
            3
        end
    end
    @test k == 2

    # try/catch tail in an assigned begin-block.
    v = begin
        1
        try
            10 ÷ 0
        catch
            -1
        end
    end
    @test v == -1

    # try/else tail: `else` runs when the try body did not throw and supplies the
    # value; `finally` never contributes the value.
    ve = begin
        try
            1
        catch
            2
        else
            3
        end
    end
    @test ve == 3
    fin = String[]
    vf = begin
        try
            10
        finally
            push!(fin, "ran")
        end
    end
    @test vf == 10
    @test fin == ["ran"]

    # An `if` with no matching branch is `nothing` (matches Julia).
    vn = begin
        1
        if false
            99
        end
    end
    @test vn === nothing

    # A loop tail keeps the `nothing` value (matches Julia).
    vl = begin
        1
        for _ in 1:3
        end
    end
    @test vl === nothing

    # #7617: non-tail assignments inside the begin-block stay visible afterward.
    outer = begin
        captured = 42
        captured + 1
    end
    @test outer == 43
    @test captured == 42
end

@testset "begin-block as value: function tail + nesting" begin
    # Function implicit return of a begin-block whose tail is control flow.
    f1(x) = begin
        y = x
        if y > 0
            y * 10
        elseif y == 0
            0
        else
            -1
        end
    end
    @test f1(5) == 50
    @test f1(0) == 0
    @test f1(-3) == -1

    # A `try` whose branch tail is itself an `if` (nested control flow in a tail).
    f2(x) = begin
        try
            if x > 0
                x * 10
            else
                0
            end
        catch
            -1
        end
    end
    @test f2(3) == 30
    @test f2(-2) == 0

    # Deeply nested if-in-if tail.
    n = begin
        if true
            if false
                1
            else
                7
            end
        else
            0
        end
    end
    @test n == 7
end

true
