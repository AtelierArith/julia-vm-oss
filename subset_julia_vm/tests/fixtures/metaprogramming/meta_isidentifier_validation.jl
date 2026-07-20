# Test Meta.isidentifier function
# Verifies identifier validation logic matches official Julia behavior.
# Key insight: Julia keywords like "if", "for" ARE valid identifiers per Meta.isidentifier.
# Only boolean literals "true" and "false" are rejected.

using Test

@testset "Meta.isidentifier" begin
    # Valid simple identifiers
    @assert Meta.isidentifier("x")
    @assert Meta.isidentifier("hello")
    @assert Meta.isidentifier("_foo")
    @assert Meta.isidentifier("foo123")
    @assert Meta.isidentifier("camelCase")
    @assert Meta.isidentifier("snake_case")

    # Valid identifiers with exclamation mark (Julia convention for mutating functions)
    @assert Meta.isidentifier("push!")
    @assert Meta.isidentifier("pop!")

    # Valid: Julia keywords ARE valid identifiers per Meta.isidentifier
    @assert Meta.isidentifier("if")
    @assert Meta.isidentifier("for")
    @assert Meta.isidentifier("function")
    @assert Meta.isidentifier("return")
    @assert Meta.isidentifier("while")
    @assert Meta.isidentifier("end")

    # Symbol form
    @assert Meta.isidentifier(:x)
    @assert Meta.isidentifier(:hello)
    @assert Meta.isidentifier(:for)

    # Invalid: empty string
    @assert !Meta.isidentifier("")

    # Invalid: starts with digit
    @assert !Meta.isidentifier("1abc")
    @assert !Meta.isidentifier("123")

    # Invalid: boolean literals (special case in Julia)
    @assert !Meta.isidentifier("true")
    @assert !Meta.isidentifier("false")

    # Invalid: starts with operator character
    @assert !Meta.isidentifier("+")
    @assert !Meta.isidentifier("-x")
    @assert !Meta.isidentifier("@foo")

    # All tests passed
    @test true
end

true
