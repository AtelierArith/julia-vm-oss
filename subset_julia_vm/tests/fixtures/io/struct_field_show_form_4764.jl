# Issue #4764: when formatting a struct via print / string for display,
# the field recursion used the print form (bare) instead of the show form
# (quoted). String / Char fields lost their quotes, producing output that
# did not round-trip and diverged visibly from upstream Julia.
#
# Fix: format_struct_instance recurses into fields via
# format_value_show_field which quotes String values (with escape) and
# single-quotes Char values, falling through to format_value for other
# variants. Mirrors upstream Julia's default struct show fallback that
# calls show on each field.

using Test

struct Foo4764
    x::Int64
    y::String
end

struct Bar4764
    c::Char
end

@testset "string(::struct) quotes String fields (Issue #4764)" begin
    f = Foo4764(42, "hi")
    @test string(f) == "Foo4764(42, \"hi\")"

    buf = IOBuffer()
    print(buf, f)
    @test String(take!(buf)) == "Foo4764(42, \"hi\")"
end

@testset "string(::struct) single-quotes Char fields (Issue #4764)" begin
    b = Bar4764('a')
    @test string(b) == "Bar4764('a')"
end

@testset "string(::Pair) uses show form for fields (Issue #4764)" begin
    p = Pair("a", "b")
    @test string(p) == "\"a\" => \"b\""
end

@testset "string(::struct) escapes special chars in String fields (Issue #4764)" begin
    g = Foo4764(1, "line1\nline2")
    # Inner newline gets the \n escape sequence in show form
    @test string(g) == "Foo4764(1, \"line1\\nline2\")"
end

@testset "string(::struct) numeric fields unchanged (Issue #4764)" begin
    # Make sure the fix doesn't regress non-String/Char field formatting
    f = Foo4764(0, "")
    @test string(f) == "Foo4764(0, \"\")"
end

true
