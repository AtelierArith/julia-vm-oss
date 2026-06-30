using Test

# Issue #6532: `throw(x)` (and the builtin `rethrow`) have return type
# `Union{}` (Bottom), mirroring upstream `add_tfunc(throw, 1, 1, ->Bottom, 0)`
# in `julia/Compiler/src/tfuncs.jl`. Consequences verified here:
#   * a function whose every exit raises infers `Union{}` — including
#     transitively through the pure-Julia `error(...)` wrappers;
#   * a raising branch contributes nothing to the join, so the non-raising
#     branch's type wins (`x > 0 ? throws() : 1.5` is `Float64`);
#   * runtime behavior is unchanged: the raised exception is still catchable
#     and the non-raising branch still returns its value.
#
# Upstream julia 1.12 reports exactly the same results.

throws_only_6532() = error("boom")
direct_throw_6532() = throw(ArgumentError("x"))
nested_caller_6532() = throws_only_6532()
branch_caller_6532(x::Int64) = x > 0 ? throws_only_6532() : 1.5
branch_throw_6532(x::Int64) = x > 0 ? 1.0 : error("neg")
function loop_throw_6532(n::Int64)
    for i in 1:n
        if i > 3
            throw(DomainError(i))
        end
    end
    return "ok"
end
function if_throw_6532(x::Int64)
    if x > 0
        return 1
    end
    error("neg")
end

@testset "always-throwing functions infer Union{} (#6532)" begin
    @test Base.infer_return_type(throws_only_6532, Tuple{}) === Union{}
    @test Base.infer_return_type(direct_throw_6532, Tuple{}) === Union{}
    @test Base.infer_return_type(nested_caller_6532, Tuple{}) === Union{}
end

@testset "raising branches join away (#6532)" begin
    @test Base.infer_return_type(branch_caller_6532, Tuple{Int64}) === Float64
    @test Base.infer_return_type(branch_throw_6532, Tuple{Int64}) === Float64
    @test Base.infer_return_type(loop_throw_6532, Tuple{Int64}) === String
    @test Base.infer_return_type(if_throw_6532, Tuple{Int64}) === Int64
end

@testset "runtime raising behavior unchanged (#6532)" begin
    @test_throws ErrorException throws_only_6532()
    @test_throws ArgumentError direct_throw_6532()
    @test_throws ErrorException branch_caller_6532(1)
    @test branch_caller_6532(-1) == 1.5
    @test branch_throw_6532(1) == 1.0
    @test loop_throw_6532(2) == "ok"
    @test_throws DomainError loop_throw_6532(5)
    @test if_throw_6532(3) == 1
end

true
