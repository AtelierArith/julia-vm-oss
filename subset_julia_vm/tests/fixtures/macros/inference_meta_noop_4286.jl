# Test inference metadata annotation macros compatibility wrappers (Issue #4286)

using Test

Base.@nospecializeinfer function f_4286(x)
    return x
end

@noinline function g_4286(x)
    return f_4286(x)
end

Base.@propagate_inbounds h_4286(x) = x + 1

@inline function inline_long_4286(x)
    return x + 3
end

Base.@inline inline_short_4286(x) = x + 4

Base.@propagate_inbounds @inline nested_inline_4286(x) = x + 5

Base.@constprop :aggressive function constprop_long_4286(x)
    return x + 6
end

Base.@constprop :none constprop_short_4286(x) = x + 7

Base.@assume_effects :foldable function assume_effects_long_4286(x)
    return x + 8
end

Base.@assume_effects :terminates_locally assume_effects_short_4286(x) = x + 9

callee_inline_expr_4286(x) = x + 1

function inline_expr_call_4286(x)
    y = @inline callee_inline_expr_4286(x)
    z = @noinline (callee_inline_expr_4286(x) + 1)
    y + z
end

function constprop_statement_4286(x)
    Base.@constprop :aggressive
    return x + 10
end

function assume_effects_statement_4286(x)
    Base.@assume_effects :foldable
    return x + 11
end

function assume_effects_callsite_4286(x)
    y = Base.@assume_effects :foldable assume_effects_long_4286(x)
    return y + 1
end

function inline_statement_marker_4286(x)
    @inline
    return x + 12
end

function noinline_statement_marker_4286(x)
    @noinline
    return x + 13
end

@inline function boundscheck_guard_4286()
    @boundscheck return 1
    return 2
end

boundscheck_caller_4286() = @inbounds boundscheck_guard_4286()

@inline function boundscheck_specialized_guard_4286(x)
    @boundscheck return x + 100
    return x + 1
end

boundscheck_specialized_caller_4286(x) = @inbounds boundscheck_specialized_guard_4286(x)

function nospecialize_param_4286(@nospecialize x)
    return x + 1
end

function specialize_param_4286(@specialize(x))
    return x + 2
end

function nospecialize_statement_4286(x, y)
    @nospecialize x y
    x + y
end

function specialize_statement_4286(x, y)
    @nospecialize x y
    @specialize
    x * y
end

@testset "inference metadata annotations parse and execute" begin
    @test g_4286(3) == 3
    Base.@boundscheck @test h_4286(2) == 3
    @test inline_long_4286(2) == 5
    @test inline_short_4286(2) == 6
    @test nested_inline_4286(2) == 7
    @test constprop_long_4286(2) == 8
    @test constprop_short_4286(2) == 9
    @test assume_effects_long_4286(2) == 10
    @test assume_effects_short_4286(2) == 11
    @test inline_expr_call_4286(2) == 7
    @test constprop_statement_4286(2) == 12
    @test assume_effects_statement_4286(2) == 13
    @test assume_effects_callsite_4286(2) == 11
    @test inline_statement_marker_4286(2) == 14
    @test noinline_statement_marker_4286(2) == 15
    @test boundscheck_guard_4286() == 1
    @test boundscheck_caller_4286() == 2
    @test boundscheck_specialized_guard_4286(1) == 101
    @test boundscheck_specialized_caller_4286(1) == 2
    @test nospecialize_param_4286(2) == 3
    @test specialize_param_4286(2) == 4
    @test nospecialize_statement_4286(2, 3) == 5
    @test specialize_statement_4286(2, 3) == 6
end

true
