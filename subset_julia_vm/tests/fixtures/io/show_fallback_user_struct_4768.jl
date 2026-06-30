# Issue #4768: `repr(user_struct)` crashed with "indexing not
# supported for I64(N)" because the Pure-Julia `show(io, x)` had
# explicit arms for built-in types (Bool, Int*, Float*, String,
# Symbol, Tuple, Pair, Dict, ...) but no fallback for user-defined
# structs. Dispatch silently fell through to a built-in arm that
# tried `x[1]`.
#
# Fix: add a generic `show(io::IO, x)` fallback that prints
# `typeof(x)(field1, field2, ...)` in show form. Specific arms
# still win via most-specific-method dispatch.

using Test

struct ShowFallbackFoo4768
    a::Int64
    b::String
end

struct ShowFallbackSingle4768
    x::Int64
end

struct ShowFallbackThree4768
    x::Int64
    y::Float64
    z::String
end

@testset "repr(user_struct) does not crash (Issue #4768)" begin
    f = ShowFallbackFoo4768(1, "hi")
    @test repr(f) == "ShowFallbackFoo4768(1, \"hi\")"

    s = ShowFallbackSingle4768(42)
    @test repr(s) == "ShowFallbackSingle4768(42)"

    t = ShowFallbackThree4768(7, 2.5, "k")
    @test repr(t) == "ShowFallbackThree4768(7, 2.5, \"k\")"
end

@testset "specific show arms still win over generic fallback (Issue #4768)" begin
    # Built-in types must keep their existing show form
    @test repr(Pair(1, 2)) == "1 => 2"
    @test repr((1, 2)) == "(1, 2)"
    @test repr([1, 2, 3]) == "[1, 2, 3]"
    @test repr("hello") == "\"hello\""
    @test repr(:sym) == ":sym"
    @test repr(1.5) == "1.5"
    @test repr(true) == "true"
    @test repr(nothing) == "nothing"
    @test repr(missing) == "missing"
end

@testset "show(io, user_struct) into IOBuffer (Issue #4768)" begin
    buf = IOBuffer()
    show(buf, ShowFallbackFoo4768(3, "x"))
    @test String(take!(buf)) == "ShowFallbackFoo4768(3, \"x\")"
end

true
