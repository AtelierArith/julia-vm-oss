# Issue #4827 (follow-up to #4761): multi-arg `print(io, a, x, b)` and
# `println(io, a, x, b)` with a statically-`IO` first arg must dispatch each
# struct value to a user-defined `show(io, ::T)` method.
#
# #4761 fixed the exact 2-arg `print(io, x)` shape (the VM's IOPrint handler
# dispatches to user `show` only for `IOPrint(2)`). Multi-arg writes were
# bundled into one `IOPrint(N)` call, which skipped that dispatch and
# field-dumped the struct (exposing internal helper fields). The compiler now
# splits a statically-`IO` multi-arg `print`/`println` into a sequence of
# two-arg `IOPrint` writes (one per value), so each value routes through user
# `show`, leaving the IO handle for the trailing newline / chaining.

using Test

struct MultiArgShow4827
    a::Int
    b::Int
    internal::Int   # helper field the user does not want exposed
end
Base.show(io::IO, x::MultiArgShow4827) = print(io, "MultiArgShow4827($(x.a), $(x.b))")

struct BoxMultiArg4827{T}
    contents::T
end
Base.show(io::IO, b::BoxMultiArg4827{T}) where {T} = print(io, "Box[", T, "]:", b.contents)

@testset "multi-arg print(io, a, x, b) dispatches user show (Issue #4827)" begin
    x = MultiArgShow4827(1, 2, 99)
    buf = IOBuffer()
    print(buf, "[", x, "]")
    @test String(take!(buf)) == "[MultiArgShow4827(1, 2)]"
end

@testset "multi-arg print(io, ...) dispatches user show repeatedly (Issue #4827)" begin
    x = MultiArgShow4827(1, 2, 99)
    buf = IOBuffer()
    print(buf, "a=", x, " b=", x, "!")
    @test String(take!(buf)) == "a=MultiArgShow4827(1, 2) b=MultiArgShow4827(1, 2)!"
end

@testset "multi-arg print(io, ...) dispatches parametric where show (Issue #4827)" begin
    b = BoxMultiArg4827(9)
    buf = IOBuffer()
    print(buf, "<", b, ">")
    @test String(take!(buf)) == "<Box[Int64]:9>"
end

@testset "multi-arg println(io, a, x, b) dispatches user show (Issue #4827)" begin
    x = MultiArgShow4827(3, 4, 99)
    buf = IOBuffer()
    println(buf, "[", x, "]")
    @test String(take!(buf)) == "[MultiArgShow4827(3, 4)]\n"
end

# --- Regression: single-arg / stdout cases must keep working (#4761/#4853) ---

@testset "single-arg print(io, x) still dispatches user show (Issue #4827)" begin
    x = MultiArgShow4827(7, 8, 99)
    buf = IOBuffer()
    print(buf, x)
    @test String(take!(buf)) == "MultiArgShow4827(7, 8)"
end

@testset "multi-arg print(io, ...) without structs concatenates (Issue #4827)" begin
    buf = IOBuffer()
    print(buf, "x=", 1, ", y=", 2)
    @test String(take!(buf)) == "x=1, y=2"
end

# Stdout single-arg print/println(x) still dispatching user show is verified by
# the parity check against upstream (these write to stdout, so they emit no
# testset summary; kept as bare script lines like println_iobuffer_user_show_4853).
let
    x = MultiArgShow4827(1, 2, 99)
    println(x)
    print(x)
    println()
end

true
