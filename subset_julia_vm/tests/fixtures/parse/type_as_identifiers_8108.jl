using Test

# Issue #8108: `type` and `as` are *contextual* keywords. `type` is significant
# only as the keyword half of `abstract type` / `primitive type`; `as` only in
# import/using aliasing. Everywhere else (function names — long & short form,
# ordinary variables, parameters, struct fields) they are plain identifiers,
# exactly as upstream Julia parses them. (Same family as `outer`, Issue #8099.)

# (1) `type` — long-form and short-form function definitions.
function type(x)
    x + 1
end
type() = 100

# (2) `as` — long-form and short-form function definitions.
function as(x)
    x * 2
end
as() = 200

# `type` / `as` as struct field names.
struct Holder
    type::Int
    as::Int
end

# `type` / `as` as parameter names.
addtype(type) = type + 1
addas(as) = as + 1

# `type` is still the keyword half of `abstract`/`primitive type`.
abstract type MyAbstract end
primitive type MyPrim 8 end

# `as` is still the alias keyword in import/using position. The alias *binding*
# is a separate, currently-unsupported feature in sjulia, so (like the existing
# `modules/using_as.jl` fixture) we only exercise the parse/lower path here and
# do not call the alias.
using Base: identity as _id_alias

@testset "`type`/`as` as ordinary identifiers (Issue #8108)" begin
    # Long-form and short-form function definitions named `type` / `as`.
    @test type(4) == 5
    @test type() == 100
    @test as(4) == 8
    @test as() == 200

    # Function values bound to variables, then called.
    f = type
    @test f(9) == 10
    g = as
    @test g(9) == 18

    # `type` / `as` as struct field names.
    @test Holder(7, 8).type == 7
    @test Holder(7, 8).as == 8

    # `type` / `as` as parameter names.
    @test addtype(10) == 11
    @test addas(10) == 11

    # `type` / `as` as ordinary local variables.
    let type = 42, as = 43
        @test type == 42
        @test as == 43
    end

    # The contextual `type` keyword still defines types.
    @test MyAbstract isa Type
    @test sizeof(MyPrim) == 1

    # The `using ... as ...` statement above parsed and lowered without error.
    @test 2 + 2 == 4
end

true
