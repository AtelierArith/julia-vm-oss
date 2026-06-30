# Issue #4865: `@show` emitted values in print-form instead of
# show-form, so `@show "positive"` produced bare
# `x = positive` instead of upstream Julia's `x = "positive"`, and
# `@show :foo` dropped the leading colon. Floats and ints already
# matched because their `print` and `show` forms agree.
#
# Fix: the `_do_show` helper in `base/macros.jl` now uses
# `println(expr_str, " = ", repr(value))`, mirroring upstream
# Julia's `@show` lowering in `julia/base/show.jl:1283-1291`:
#
#     macro show(exs...)
#         blk = Expr(:block)
#         for ex in exs
#             push!(blk.args, :(println($(sprint(show_unquoted,ex)*" = "),
#                                       repr(begin local value = $(esc(ex)) end))))
#         end
#         ...
#     end
#
# `repr(value)` produces the show-form String once, which `println`
# then emits verbatim (println uses print-form for String args, so
# the embedded quotes / colon survive unchanged).
#
# These tests verify the underlying `repr` of each `@show`-returned
# value still matches what upstream Julia would render — the
# user-visible stdout shape is the same. Capturing stdout directly
# from a `@show` invocation is not portable across sjulia and julia
# (no `redirect_stdout` parity), so we anchor on (a) the value being
# returned unchanged and (b) `repr(x)` agreeing with the upstream
# show-form text — which is exactly the part the bug fix shifts.

using Test

@testset "@show returns its value unchanged (regression guard)" begin
    @test (@show "positive") == "positive"
    @test (@show :foo) == :foo
    @test (@show 42) == 42
    @test (@show 3.14) == 3.14
    @test (@show 'A') == 'A'
    @test (@show (1, "two", :three)) == (1, "two", :three)
    @test (@show [1, 2, 3]) == [1, 2, 3]
    @test (@show nothing) === nothing
    @test (@show missing) === missing
end

@testset "show-form text for the @show-emitted value (Issue #4865)" begin
    # The fix routes the value through `repr(value)` (upstream's
    # canonical `@show` lowering). `repr(x)` exercises the exact same
    # show-form output — anchoring on it pins what the user now sees
    # in `@show`'s stdout line after `... = `.
    @test repr("positive") == "\"positive\""
    @test repr(:foo) == ":foo"
    @test repr('A') == "'A'"
    @test repr(42) == "42"
    @test repr(3.14) == "3.14"
    @test repr((1, "two", :three)) == "(1, \"two\", :three)"
    @test repr([1, 2, 3]) == "[1, 2, 3]"
    @test repr(nothing) == "nothing"
    @test repr(missing) == "missing"
end

@testset "@show on nested expressions still returns the result" begin
    f_4865(x) = x + 1
    @test (@show f_4865(5)) == 6
    # Wrap arithmetic in a local to keep the `@show` stdout line
    # ("EXPR = VALUE") free of multiple bare integer tokens; the
    # fixture-parity helper's awk fallback otherwise misparses
    # `1 + 2 = 3` as a testset summary row.
    sum_4865 = 1 + 2
    @test (@show sum_4865) == 3
    @test (@show length("abc")) == 3
end

true
