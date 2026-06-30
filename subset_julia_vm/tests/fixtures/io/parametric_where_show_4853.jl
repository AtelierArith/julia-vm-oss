# Issue #4853: a parametric `Base.show(io::IO, b::Box{T}) where T` method must
# dispatch from every print/string/show entry point, not just `repr`.
#
# The signature's second-param JuliaType is `Struct("Box{T}")` (the type-var
# form), so the show-method registration in compile/mod.rs now also registers
# the bare base name ("Box") that the runtime lookup (`user_show_method_for`)
# falls back to. Without it, `print(io, x)`, `string(x)` and the stdout print
# paths all field-dumped the struct (`Box{Int64}(42)`) while only `repr`
# dispatched correctly (it routes through the Pure-Julia method table).
#
# Upstream Julia produces "Box[Int64]:42" from every path.

using Test

struct BoxWhere4853{T}
    contents::T
end

Base.show(io::IO, b::BoxWhere4853{T}) where {T} = print(io, "Box[", T, "]:", b.contents)

@testset "repr dispatches parametric where show (Issue #4853)" begin
    b = BoxWhere4853(42)
    @test repr(b) == "Box[Int64]:42"
end

@testset "string dispatches parametric where show (Issue #4853)" begin
    b = BoxWhere4853(42)
    @test string(b) == "Box[Int64]:42"
end

@testset "print(buf, ::Box{T}) dispatches parametric where show (Issue #4853)" begin
    b = BoxWhere4853(42)
    buf = IOBuffer()
    print(buf, b)
    @test String(take!(buf)) == "Box[Int64]:42"
end

@testset "sprint(show, ::Box{T}) dispatches parametric where show (Issue #4853)" begin
    b = BoxWhere4853(42)
    @test sprint(show, b) == "Box[Int64]:42"
end

@testset "parametric where show keeps T for other element types (Issue #4853)" begin
    bf = BoxWhere4853(3.5)
    @test string(bf) == "Box[Float64]:3.5"
    bs = BoxWhere4853("hi")
    @test repr(bs) == "Box[String]:hi"
end

@testset "all paths agree for parametric where show (Issue #4853)" begin
    b = BoxWhere4853(7)
    buf = IOBuffer()
    print(buf, b)
    printed = String(take!(buf))
    @test printed == repr(b)
    @test printed == string(b)
    @test printed == sprint(show, b)
end

true
