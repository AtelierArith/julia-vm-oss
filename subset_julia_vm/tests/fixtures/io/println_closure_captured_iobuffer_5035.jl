# Issue #5035: `print`/`println(buf, ...)` inside a lambda/closure where `buf`
# is a captured *top-level* (global) IOBuffer() must route to the IO sink, not
# field-dump the buffer value (`IOBuffer(...)`) to stdout.
#
# Root cause: a module-level `buf = IOBuffer()` was registered in `global_types`
# via `infer_value_type_with_structs`, which lacked an `IOBuffer` -> IO case and
# fell back to `I64`. Inside a closure the captured `buf` resolves its type from
# `global_types` (not `locals`), so `infer_expr_type(buf)` returned a concrete,
# non-IO type. The `println(io, ...)` / `print(io, ...)` lowering therefore
# skipped the IO path and took the stdout fallback, printing the buffer itself
# instead of writing to the sink. Top-level (non-closure) calls and calls inside
# a function/@testset worked because the local `buf` was typed `IO` in `locals`.
# The fix maps `IOBuffer()` to `IO` in `infer_value_type_with_structs` so captured
# top-level IOBuffers route correctly.
#
# NOTE: these tests intentionally keep `buf` as a *global* (top-level binding),
# NOT inside a @testset, because @testset would make `buf` a local and mask the
# bug (which depends on the global_types lookup path).

using Test

# Closure captures global IOBuffer, single value.
buf_single_5035 = IOBuffer()
f_single_5035 = x -> println(buf_single_5035, x)
f_single_5035(10)
@test String(take!(buf_single_5035)) == "10\n"

# Closure captures global IOBuffer, multiple values.
buf_multi_5035 = IOBuffer()
f_multi_5035 = x -> println(buf_multi_5035, x, x + 1)
f_multi_5035(10)
@test String(take!(buf_multi_5035)) == "1011\n"

# print (no newline), multiple values, captured global IOBuffer.
buf_print_5035 = IOBuffer()
g_print_5035 = () -> print(buf_print_5035, "a", "b", "c")
g_print_5035()
@test String(take!(buf_print_5035)) == "abc"

# Closure captures both the global IOBuffer and other global vars.
buf_vars_5035 = IOBuffer()
a_5035 = 1
b_5035 = 2
h_vars_5035 = () -> println(buf_vars_5035, a_5035, b_5035)
h_vars_5035()
@test String(take!(buf_vars_5035)) == "12\n"

# Regression: function/@testset-scoped (local) IOBuffer keeps working.
@testset "local IOBuffer closure still works (Issue #5035)" begin
    function outer5035()
        buf = IOBuffer()
        inner = () -> println(buf, 7, 8)
        inner()
        return String(take!(buf))
    end
    @test outer5035() == "78\n"

    function make5035(io)
        return () -> println(io, "x", "y")
    end
    buf2 = IOBuffer()
    make5035(buf2)()
    @test String(take!(buf2)) == "xy\n"
end

true
