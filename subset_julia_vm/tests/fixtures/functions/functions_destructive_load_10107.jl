# Issue #10107: the peephole optimizer rewrites a verbatim-clone `LoadSlot*(i)`
# immediately followed by a value-consuming `Return*` into a moving
# `TakeSlot(i)` (destructive load) inside function bodies where no exception
# handler is active. This fixture pins END-TO-END CORRECTNESS across the cases
# where the rewrite fires AND the adversarial cases where a naive rewrite would
# be wrong, so the liveness/handler guard cannot silently regress.
#
# The optimization is invisible to output — it only avoids a clone — so every
# assertion below must match upstream `julia` verbatim.

using Test

# --- Cases where the rewrite SHOULD fire (heap-carrying return values) -------

# Trivial accessor: LoadSlotArray(0); ReturnArray -> TakeSlot(0); ReturnArray.
ident_arr_10107(v) = v

# Heap value built then returned via a slot on both branches of an if — the two
# `LoadSlot*; Return*` returns are mutually exclusive, so each is a dynamic last
# use and both may be moved.
function branch_ret_10107(a, b, c)
    x = [a, b, c]
    if a + b > 0
        return x
    end
    return x
end

# String / dict / tuple accessors (each exercises a distinct heap-typed load).
str_id_10107(s) = s
dict_id_10107(d) = d
tup_id_10107(t) = t

# --- ADVERSARIAL: a naive move would corrupt output; the guard must decline ---

# try/finally where the finally reads the SAME variable that is returned. The
# value-returning load sits under an active handler, so the move must be
# declined: emptying the slot before `finally` runs would lose the value.
function try_finally_reads_var_10107(x)
    log = String[]
    r = try
        return_marker(x, log)
    finally
        push!(log, "finally saw len=$(length(x))")
    end
    return (r, log)
end
return_marker(x, log) = (push!(log, "body saw len=$(length(x))"); x)

# Closure captures a variable that is subsequently returned. Moving the value
# out of the slot on return must not disturb the closure's captured handle
# (captures are independent references to the same heap object).
function closure_then_return_10107(v)
    seen = Int[]
    g = () -> push!(seen, length(v))
    g()
    g()
    return (v, seen)
end

# Aliasing: an escaping closure shares the same array the caller returns. After
# the destructive return, the returned array and the closure's captured array
# must still be the same object (mutation through one is visible via the other).
function aliased_escape_10107()
    x = [1, 2, 3]
    pusher = () -> push!(x, 99)
    return (x, pusher)
end

@testset "destructive load 10107" begin
    # Rewrite-fires cases: value returned verbatim.
    @test ident_arr_10107([9, 8, 7]) == [9, 8, 7]
    @test branch_ret_10107(1, 2, 3) == [1, 2, 3]
    @test branch_ret_10107(-5, 1, 2) == [-5, 1, 2]
    @test str_id_10107("hello") == "hello"
    @test dict_id_10107(Dict(:a => 1)) == Dict(:a => 1)
    @test tup_id_10107((1, "two", [3])) == (1, "two", [3])

    # Adversarial: try/finally reading the returned var still sees it.
    r, log = try_finally_reads_var_10107([10, 20])
    @test r == [10, 20]
    @test log == ["body saw len=2", "finally saw len=2"]

    # Adversarial: closure capture unaffected by the destructive return.
    v, seen = closure_then_return_10107([1, 2, 3, 4])
    @test v == [1, 2, 3, 4]
    @test seen == [4, 4]

    # Adversarial: aliasing preserved — returned array and captured array are
    # the same object.
    arr, pusher = aliased_escape_10107()
    @test arr == [1, 2, 3]
    pusher()
    @test arr == [1, 2, 3, 99]
end

true
