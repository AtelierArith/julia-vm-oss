# Issue #9127: a FILTERED generator with a non-trivial body, and a generator
# with a tuple-destructuring binding, must be LAZY — side effects fire at
# iteration/collect time, never at construction time — matching upstream Julia.
# Previously both shapes hit the eager comprehension fallback (side effects
# fired when the generator value was constructed).
#
# The ordering testsets record side effects into a MODULE-GLOBAL `glog`. The
# function-scope sub-cases — a filtered generator whose lifted body/predicate
# live in a function scope (locals) and/or CAPTURE an enclosing local — are now
# lazy too (Issue #9271); their ordering is asserted in
# filtered_generator_function_scope_lazy_9271.jl. The two function-scope
# testsets below keep asserting correct values as an additional guard.

using Test

glog = String[]
logstep(tag) = (push!(glog, tag); nothing)

@testset "filtered non-trivial body: side effects fire at collect, not construction" begin
    empty!(glog)
    g = (begin
        logstep("computing $x")
        x^2
    end for x in 1:4 if x % 2 == 0)
    @test isempty(glog)                          # construction ran nothing
    @test collect(g) == [4, 16]
    @test glog == ["computing 2", "computing 4"] # only kept elements, in order
end

@testset "tuple-destructuring binding: lazy, no filter" begin
    empty!(glog)
    pairs = [(1, 2), (3, 4), (5, 6)]
    g = (begin
        logstep("computing $a,$b")
        a + b
    end for (a, b) in pairs)
    @test isempty(glog)
    @test collect(g) == [3, 7, 11]
    @test glog == ["computing 1,2", "computing 3,4", "computing 5,6"]
end

@testset "tuple-destructuring binding with filter: lazy" begin
    empty!(glog)
    pairs = [(1, 2), (3, 4), (5, 6)]
    g = (begin
        logstep("computing $a,$b")
        a + b
    end for (a, b) in pairs if a > 1)
    @test isempty(glog)
    @test collect(g) == [7, 11]
    @test glog == ["computing 3,4", "computing 5,6"]
end

@testset "filtered / tuple generators feed consumers correctly" begin
    @test collect(x^2 for x in 1:4 if x % 2 == 0) == [4, 16]
    @test sum(x^2 for x in 1:10 if x % 3 == 0) == 126
    @test first(x^2 for x in 2:6 if x > 3) == 16
    @test count(true for _ in 1:5 if true) == 5
    @test collect(a + b for (a, b) in [(1, 2), (3, 4)]) == [3, 7]
    @test collect(a * b for (a, b) in [(1, 2), (3, 4)] if a + b > 3) == [12]
end

@testset "plain filtered generator (both f(var)/p(var)) still lazy and correct" begin
    sq(x) = x * x
    iseven2(x) = x % 2 == 0
    @test collect(sq(x) for x in 1:6 if iseven2(x)) == [4, 16, 36]
end

@testset "captured-predicate filtered generator: now lazy and correct (Issue #9271)" begin
    # The predicate `x > k` captures `k`; the sub-case is now lazy via a
    # runtime-callable FilteredRuntimeValue (Issue #9271): the body fires at
    # collect time, after `constructed`.
    empty!(glog)
    function keep_big(data, k)
        g = (begin
            logstep("big $x")
            x * 10
        end for x in data if x > k)
        logstep("constructed")
        collect(g)
    end
    @test keep_big([1, 2, 3, 4], 2) == [30, 40]
    @test glog == ["constructed", "big 3", "big 4"]
end

@testset "filtered generator inside a function (zero captures): now lazy (Issue #9271)" begin
    # Even with ZERO captures, the lifted __gen_body_N / __gen_pred_N land in
    # self.locals (function scope), so this used to be eager; it is now lazy
    # (Issue #9271).
    empty!(glog)
    function squares_of_evens()
        g = (begin
            logstep("sq $x")
            x^2
        end for x in 1:6 if x % 2 == 0)
        logstep("constructed")
        collect(g)
    end
    @test squares_of_evens() == [4, 16, 36]
    @test glog == ["constructed", "sq 2", "sq 4", "sq 6"]
end

@testset "Dict tuple-destructuring generator: lazy base, collects/consumes (Issue #9127)" begin
    # Regression: lifting a tuple-destructuring generator routed it onto the lazy
    # MakeGenerator path, whose synchronous iterate could not drive a Dict (whose
    # `iterate` is pure Julia), so `collect(k + v for (k, v) in Dict(...))` errored
    # with `unsupported iterator type Dict{...}`. The base is now materialized once
    # at construction (finite, side-effect-free); the mapped body stays lazy.
    empty!(glog)
    d = Dict(1 => 10, 2 => 20, 3 => 30)
    g = (begin
        logstep("kv")
        k + v
    end for (k, v) in d)
    @test isempty(glog)                       # base traversal fires no body side effect
    @test sort(collect(g)) == [11, 22, 33]    # (Dict iteration order is unspecified)
    @test length(glog) == 3                   # body fired once per entry at collect time
    # scalar generators over the Dict's key/value iterators also collect
    @test sort(collect(v * 2 for v in values(d))) == [20, 40, 60]
    @test sort(collect(k * 10 for k in keys(d))) == [10, 20, 30]
    # filtered tuple-destructuring over a Dict
    @test sort(collect(k + v for (k, v) in d if v > 15)) == [22, 33]
    # non-`collect` consumers drive the same materialized base
    @test sum(k + v for (k, v) in d) == 66
    @test count(v > 15 for (k, v) in d) == 2
    @test first(k + v for (k, v) in Dict(5 => 50)) == 55
end

@testset "empty filtered collect preserves the inferred eltype (Issue #9127)" begin
    # Regression: a filtered generator whose predicate removed EVERY element
    # collapsed to `Union{}[]` instead of the inferred `Int64[]`, because the
    # runtime FilterMap finalizer ignored the statically-inferred element type
    # when the filtered result was empty.
    a = collect(x^2 for x in 1:4 if x > 100)
    @test isempty(a)
    @test eltype(a) == Int64                  # not Union{}
    # a plain-callable body still keeps the static eltype when the predicate is
    # syntactically transparent
    sqr(x) = x * x
    e = collect(sqr(x) for x in 1:4 if x > 100)
    @test isempty(e)
    @test eltype(e) == Int64
    # non-empty control keeps its eltype
    b = collect(x^2 for x in 1:4 if x % 2 == 0)
    @test b == [4, 16]
    @test eltype(b) == Int64
end

true
