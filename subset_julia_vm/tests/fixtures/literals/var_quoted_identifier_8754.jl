# var"..." non-standard identifier syntax (Issue #8754)
#
# `var"name"` names a binding by the quoted string's exact content,
# including spaces and otherwise-reserved spellings. JuliaSyntax merges
# the `var` prefix and the string into a single identifier token; sjulia
# mirrors this by merging the parsed prefixed-string into an Identifier
# CST leaf spanning only the quoted content.

using Test

@testset "var quoted identifiers" begin
    # Plain assignment / read-back
    var"x" = 1
    @test var"x" == 1
    @test x == 1

    # Identifier containing a space
    var"dict key" = 41
    var"dict key" += 1
    @test var"dict key" == 42

    # Short-form function definition with a var-named parameter
    f(var"my weird name") = var"my weird name" + 1
    @test f(41) == 42

    # Long-form function with a var-named parameter
    function g(var"a b")
        var"a b" * 2
    end
    @test g(21) == 42

    # var-named function
    var"h func"(x) = x - 1
    @test var"h func"(43) == 42

    # Symbol form: :var"..." is the Symbol of the content
    @test :var"@q" == Symbol("@q")
    @test :var"x" == :x

    # Struct with a var name and construction through the var spelling
    struct var"S t"
        a::Int
    end
    s = var"S t"(7)
    @test s.a == 7

    # var spelling of an ordinary name refers to the same binding
    q = 5
    @test var"q" == 5
    var"q" = 6
    @test q == 6

    # `var` itself remains usable as an ordinary identifier
    var = 3
    @test var + 1 == 4
end

true
