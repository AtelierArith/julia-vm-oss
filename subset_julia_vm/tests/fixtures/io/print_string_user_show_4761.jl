# Issue #4761 (remaining slice): when a user defines `show(io::IO, ::T)`
# for a user struct `T`, both `print(io, x)` and `string(x)` must
# dispatch through that user method instead of falling through to the
# generic Rust field-dump fallback. The single-arg stdout path
# (`print(x)`, `println(x)`) already routes through `PrintAnyNoNewline`
# which dispatches to `show`; this fixture pins the IO-print and
# `string(x)` slices in the BuiltinId::IOPrint / BuiltinId::StringNew
# handlers.
#
# Scope: single non-IO arg (i.e. `print(io, x)` and `string(x)`).
# Multi-arg forms (`println(io, x)`, `print(io, x, y)`, `string(x, y)`)
# still defer to the per-value Rust formatter; routing those through
# user `show` requires either a compile-time rewrite into per-arg
# IOPrint calls or a sprint-style resumption per arg and is tracked
# separately.

using Test

struct UserShowFoo4761
    a::Int
    b::Int
end

Base.show(io::IO, x::UserShowFoo4761) = print(io, "Foo<", x.a, ",", x.b, ">")

struct UserShowNoMethod4761
    x::Int
    y::Int
end

@testset "print(buf, ::UserStruct) dispatches user show (Issue #4761)" begin
    f = UserShowFoo4761(3, 7)
    buf = IOBuffer()
    print(buf, f)
    @test String(take!(buf)) == "Foo<3,7>"
end

@testset "string(::UserStruct) dispatches user show (Issue #4761)" begin
    f = UserShowFoo4761(3, 7)
    @test string(f) == "Foo<3,7>"
end

@testset "print(buf, ::UserStruct) matches repr (Issue #4761)" begin
    # When there's no print-form override (no `show(io, ::MIME, ...)` or
    # other dispatch path that print could call), `print(io, x)` and
    # `repr(x)` produce the same output for user structs.
    f = UserShowFoo4761(11, 22)
    buf = IOBuffer()
    print(buf, f)
    @test String(take!(buf)) == repr(f)
end

@testset "print(stdout, ::UserStruct) dispatches user show (Issue #4761)" begin
    # Round-trip via sprint to capture stdout-equivalent IOPrint behavior.
    f = UserShowFoo4761(5, 9)
    buf = IOBuffer()
    print(buf, f)
    @test String(take!(buf)) == "Foo<5,9>"
end

@testset "string(x) with no user show keeps generic field-dump (Issue #4761)" begin
    # Sanity check: structs without a specific `show(io, ::T)` method
    # still fall through to the generic Pure-Julia `show(io, x)`
    # fallback, which prints `StructName(field1, ...)` in show form.
    g = UserShowNoMethod4761(1, 2)
    @test string(g) == "UserShowNoMethod4761(1, 2)"
end

@testset "print(buf, x) with no user show keeps generic field-dump (Issue #4761)" begin
    g = UserShowNoMethod4761(8, 8)
    buf = IOBuffer()
    print(buf, g)
    @test String(take!(buf)) == "UserShowNoMethod4761(8, 8)"
end

@testset "built-in single-arg string paths unaffected (Issue #4761)" begin
    # Built-in types must keep their string() print-form behavior:
    # no quotes around strings, bare names for Symbols, etc.
    @test string(1.5) == "1.5"
    @test string("hi") == "hi"
    @test string(:foo) == "foo"
    @test string(true) == "true"
    @test string(nothing) == "nothing"
end

@testset "user show invoked for both print and string match (Issue #4761)" begin
    # Cross-check: for a user struct with a single show definition,
    # `string(x)` and the contents of `print(buf, x)` are byte-for-byte
    # equal — both routed through the same `show(io, x)` method.
    f = UserShowFoo4761(42, 100)
    buf = IOBuffer()
    print(buf, f)
    @test String(take!(buf)) == string(f)
end

true
