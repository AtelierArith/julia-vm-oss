# Issue #9271: a FILTERED generator whose lifted body/predicate live in a
# FUNCTION SCOPE (so `__gen_body_N` / `__gen_pred_N` are locals) and/or CAPTURE
# an enclosing local must be LAZY — side effects fire at collect/iteration time,
# never at construction — matching upstream Julia. Previously these hit the
# eager comprehension fallback (values were correct, but the body/predicate ran
# at construction), because the lazy `FilteredFunctionIndex` path could only
# carry bare function indices and would drop the captured environment. The fix
# adds a runtime-callable `FilteredRuntimeValue` map/predicate pair (mirroring
# the unfiltered `MakeGeneratorRuntime` path from Issue #9103).

using Test

# (1) CAPTURE case: the filter `x > k` captures `k`, and the body captures the
# enclosing `log`. Ordering must be `constructed` BEFORE any `body ...`.
function run_capture(data, k, log)
    g = (begin
        push!(log, "body $x")
        x * 10
    end for x in data if x > k)
    push!(log, "constructed")
    return collect(g)
end

@testset "captured filtered generator is lazy (Issue #9271)" begin
    log = String[]
    vals = run_capture([1, 2, 3, 4], 2, log)
    @test vals == [30, 40]
    @test log == ["constructed", "body 3", "body 4"]
end

# (2) ZERO-CAPTURE, function scope: the body references only a module global and
# the loop var; the predicate references only the loop var. No captures, but the
# lifted functions still land in `self.locals` (function scope), so this used to
# be eager too.
gzlog = String[]
function run_zero()
    g = (begin
        push!(gzlog, "body $x")
        x^2
    end for x in 1:4 if x % 2 == 0)
    push!(gzlog, "constructed")
    return collect(g)
end

@testset "zero-capture function-scope filtered generator is lazy (Issue #9271)" begin
    empty!(gzlog)
    vals = run_zero()
    @test vals == [4, 16]
    @test gzlog == ["constructed", "body 2", "body 4"]
end

# (3) Lazy iteration (`for`) over such a generator must also fire side effects
# lazily and in order.
function run_for(data, k, log)
    g = (begin
        push!(log, "body $x")
        x * 10
    end for x in data if x > k)
    push!(log, "constructed")
    total = 0
    for v in g
        total += v
    end
    return total
end

@testset "captured filtered generator iterates lazily via for (Issue #9271)" begin
    log = String[]
    total = run_for([1, 2, 3, 4], 2, log)
    @test total == 70
    @test log == ["constructed", "body 3", "body 4"]
end

# (4) Non-`collect` consumers drive the same lazy runtime-callable generator.
@testset "function-scope filtered generator feeds sum/count/first (Issue #9271)" begin
    function consume(data, k)
        (sum(x * 10 for x in data if x > k),
         count(x > k for x in data),
         first(x * 10 for x in data if x > k))
    end
    s, c, f = consume([1, 2, 3, 4], 2)
    @test s == 70
    @test c == 2
    @test f == 30
end

# (5) An all-filtered-out function-scope generator still reports the inferred
# eltype (`Int64[]`), not `Union{}[]`.
function empty_evens()
    collect(x^2 for x in 1:4 if x > 100)
end

@testset "empty function-scope filtered collect keeps eltype (Issue #9271)" begin
    a = empty_evens()
    @test isempty(a)
    @test eltype(a) == Int64
end

# (6) Values stay correct across a range of function-scope filtered shapes.
@testset "function-scope filtered generator values correct (Issue #9271)" begin
    function keep_big(data, k)
        collect(x * 10 for x in data if x > k)
    end
    @test keep_big([1, 2, 3, 4], 2) == [30, 40]
    @test keep_big([5, 1, 9, 2], 3) == [50, 90]

    function squares_of_evens()
        collect(x^2 for x in 1:6 if x % 2 == 0)
    end
    @test squares_of_evens() == [4, 16, 36]
end

true
