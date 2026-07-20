# Short-form function bodies can contain value-producing global declarations.

using Test

short_global_plain_9817() = (global plain_9817 = 42)
short_global_no_parens_9817() = global no_parens_9817 = 43

compound_9817 = 0
short_global_compound_9817() = (global compound_9817 += 5)

@testset "short-form global body declarations (Issue #9817)" begin
    @test short_global_plain_9817() == 42
    @test plain_9817 == 42

    @test short_global_no_parens_9817() == 43
    @test no_parens_9817 == 43

    @test short_global_compound_9817() == 5
    @test compound_9817 == 5
    @test short_global_compound_9817() == 10
    @test compound_9817 == 10

    function long_form_control_9817()
        global long_form_9817 = 44
    end
    # Long-form implicit return of the assignment value is tracked separately
    # by Issue #10023; #9817 only needs the module binding control.
    long_form_control_9817()
    @test long_form_9817 == 44
end

true
