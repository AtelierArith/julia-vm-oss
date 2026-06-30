# Issue #8212: a failed `convert(T, x)` must throw an `InexactError` exception
# OBJECT, so `catch e` binds `typeof(e) == InexactError` / `isa(e, InexactError)`
# — previously the caught value was a bare `String`. The bug is general to
# `convert` (direct call, typed local, and `for i::T in itr` loop vars), not
# specific to any one form.
using Test

@testset "catchable InexactError (Issue #8212)" begin
    # (1) direct convert call
    e1 = try; convert(Int64, 1.5); catch e; e; end
    @test e1 isa InexactError
    @test typeof(e1) == InexactError
    @test isa(e1, Exception)
    @test sprint(showerror, e1) == "InexactError: Int64(1.5)"

    # (2) typed local assignment
    function f8212()
        x::Int64 = 1.5
        x
    end
    e2 = try; f8212(); catch e; e; end
    @test e2 isa InexactError
    @test typeof(e2) == InexactError

    # (3) typed for-loop variable (#8208)
    function g8212()
        for i::Int64 in [1.5]
        end
    end
    e3 = try; g8212(); catch e; e; end
    @test e3 isa InexactError
    @test typeof(e3) == InexactError

    # narrowing / Bool conversions also raise a catchable InexactError
    e4 = try; convert(Int8, 300); catch e; e; end
    @test e4 isa InexactError
    e5 = try; convert(Bool, 2); catch e; e; end
    @test e5 isa InexactError
    @test sprint(showerror, e5) == "InexactError: Bool(2)"
end
true
