# Issue #7172: a `Base.show(io, ::T)` defined inside a module must be dispatched
# by print/println/string for values of that module's type, even though the
# value's type name is module-qualified ("Widgets.Gadget").
using Test

module Widgets
export Gadget, make_gadget

struct Gadget
    id::Int
end

# constructor helper avoids the module-qualified-constructor path
make_gadget(id) = Gadget(id)

Base.show(io::IO, g::Gadget) = print(io, "Gadget#", g.id)
end

using .Widgets

@testset "Issue #7172: module Base.show is dispatched" begin
    g = make_gadget(42)
    @test string(g) == "Gadget#42"
    # exported unqualified constructor + custom show
    @test string(Gadget(7)) == "Gadget#7"
end

true
