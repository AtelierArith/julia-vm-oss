using Test

struct CallDynamicBox3910{T}
    x::T
end

function call_dynamic_resolver_scoring_3910(x::Any)
    :any
end

function call_dynamic_resolver_scoring_3910(x::CallDynamicBox3910)
    :bare
end

function call_dynamic_resolver_scoring_3910(x::CallDynamicBox3910{T}) where {T}
    :parametric
end

function call_dynamic_resolver_scoring_3910(x::CallDynamicBox3910{Int64})
    :exact
end

function call_dynamic_resolver_scoring_3910_via_any(x)
    y::Any = x
    call_dynamic_resolver_scoring_3910(y)
end

struct CallDynamicBare3910
    x::Int64
end

function call_dynamic_resolver_bare_3910(x::Any)
    :any
end

function call_dynamic_resolver_bare_3910(x::CallDynamicBare3910)
    :bare
end

function call_dynamic_resolver_bare_via_any_3910(x)
    y::Any = x
    call_dynamic_resolver_bare_3910(y)
end

@testset "CallDynamic resolver scoring (Issue #3910)" begin
    @test call_dynamic_resolver_scoring_3910_via_any(CallDynamicBox3910{Int64}(1)) == :exact
    @test call_dynamic_resolver_scoring_3910_via_any(CallDynamicBox3910{Float64}(1.0)) == :parametric
    @test call_dynamic_resolver_bare_via_any_3910(CallDynamicBare3910(1)) == :bare
end

true
