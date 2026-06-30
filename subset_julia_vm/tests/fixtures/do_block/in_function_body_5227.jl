# do-block trailing closure attached to a call inside a function body (Issue #5227).
# Previously the do-block lambda was silently dropped during lowering when the call
# appeared inside `function ... end` / `f() = ...`, because the no-LambdaContext
# lowering path never inspected the DoClause node. It now desugars to
# `f(x -> body, args...)` (closure prepended as the FIRST argument), matching upstream.

using Test

# map do-block inside a function body
function do_map_in_fn(v)
    map(v) do x
        x * 2
    end
end

# foreach do-block inside a function body (mutates an outer Ref captured by the closure)
function do_foreach_in_fn(v)
    total = Ref(0)
    foreach(v) do x
        total[] = total[] + x
    end
    total[]
end

# get! do-block inside a function body, capturing outer var n
function do_get_in_fn(n)
    cache = Dict{Int,Int}()
    get!(cache, n) do
        n * n
    end
end

# filter do-block inside a function body
function do_filter_in_fn(v)
    filter(v) do x
        x > 2
    end
end

# do-block inside a NESTED user function body (the inner function body also goes
# through the no-LambdaContext lowering path).
# Note: a do-block whose body *itself* contains another HOF call (e.g. map inside a
# map do-block) hits an orthogonal, pre-existing nested-HOF dispatch limitation that
# affects the explicit-arrow form identically — tracked separately in Issue #5229.
function do_nested_user_fn(v)
    function inner(w)
        map(w) do x
            x + 1
        end
    end
    inner(v)
end

# multi-statement do-body capturing an outer parameter
function do_capture_in_fn(v, base)
    map(v) do x
        y = x * base
        y + 1
    end
end

# short-form `f() = ...` body with a do-block
do_short_form_in_fn(v) = map(v) do x
    x + 1
end

@testset "do-block inside function body (Issue #5227)" begin
    @test do_map_in_fn([1, 2, 3]) == [2, 4, 6]
    @test do_foreach_in_fn([1, 2, 3, 4]) == 10
    @test do_get_in_fn(4) == 16
    @test do_filter_in_fn([1, 2, 3, 4]) == [3, 4]
    @test do_nested_user_fn([1, 2, 3]) == [2, 3, 4]
    @test do_capture_in_fn([1, 2, 3], 10) == [11, 21, 31]
    @test do_short_form_in_fn([1, 2, 3]) == [2, 3, 4]
end

# Top-level do-block regression: the LambdaContext path must still work.
@testset "do-block at top level regression (Issue #5227)" begin
    r = map([1, 2, 3]) do x
        x * 2
    end
    @test r == [2, 4, 6]
end

true  # Test passed
