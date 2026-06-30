# Issue #4853 (follow-up): `println(buf, x)` where `buf` is a *local* IOBuffer()
# variable (statically typed `Any`, not `IO`) must dispatch to a user-defined
# `show(io, ::T)` method, matching `print(buf, x)` and upstream Julia.
#
# Before the fix, the single-value `println(io, x)` lowering bundled the trailing
# "\n" into one three-arg IOPrint call (`[io, x, "\n"]`) for the `Any`-typed
# first-arg case, which skips the two-arg user-`show` dispatch and field-dumps
# the struct (`Box{Int64}(42)`). The statically-`IO` case already worked via a
# Dup-based split, but a local `IOBuffer()` infers as `Any`. The fix routes the
# single-value `println(io_or_val, x)` through a dedicated IOPrintln builtin that
# discriminates the first arg's runtime kind: IO -> user-`show` + newline to the
# sink; non-IO -> print all values + newline to stdout (so `println(a, b)` with
# an `Any`-typed non-IO `a` still concatenates to stdout).

using Test

struct PlainShow4853
    x::Int
end
Base.show(io::IO, p::PlainShow4853) = print(io, "Plain<", p.x, ">")

struct BoxShow4853{T}
    contents::T
end
Base.show(io::IO, b::BoxShow4853{T}) where {T} = print(io, "Box[", T, "]:", b.contents)

@testset "println(local IOBuffer, ::T) dispatches non-parametric user show (Issue #4853)" begin
    p = PlainShow4853(5)
    buf = IOBuffer()
    println(buf, p)
    @test String(take!(buf)) == "Plain<5>\n"
end

@testset "println(local IOBuffer, ::Box{T}) dispatches parametric where show (Issue #4853)" begin
    b = BoxShow4853(9)
    buf = IOBuffer()
    println(buf, b)
    @test String(take!(buf)) == "Box[Int64]:9\n"
end

@testset "println(local IOBuffer, x) keeps non-show values working (Issue #4853)" begin
    buf = IOBuffer()
    println(buf, 42)
    @test String(take!(buf)) == "42\n"
    buf2 = IOBuffer()
    println(buf2, "hello")
    @test String(take!(buf2)) == "hello\n"
end

@testset "println(buf, a, b) with multiple values still concatenates (Issue #4853)" begin
    # Multi-value println(io, ...) must keep concatenating every argument; the
    # single-value IOPrintln split only applies to exactly `println(io, x)`.
    a = 1
    b = 2
    buf = IOBuffer()
    println(buf, a, b)
    @test String(take!(buf)) == "12\n"
end

# Stdout concatenation for a non-IO `Any`-typed first arg is covered by the
# parity check (println(a, b) -> "12\n"); see the script-level lines below.
let
    a = 1
    b = 2
    println(a, b)
end

true
