# Caught exceptions expose upstream's payload fields instead of rendered
# strings (Issue #11374, tech-debt #11399): BoundsError carries the actual
# container and the complete index tuple; MethodError carries the callable
# and the argument values for compile-time-detected dispatch misses and the
# named numeric fast paths.
using Test

f11374(x::Int) = x
wrap_sqrt11374(x) = sqrt(x)

@testset "BoundsError payload (Issue #11374)" begin
    a = [1, 2, 3]
    e1 = try
        a[10]
    catch e
        e
    end
    @test e1 isa BoundsError
    @test e1.a == a
    @test e1.i == (10,)

    m = zeros(2, 2)
    e2 = try
        m[9, 9]
    catch e
        e
    end
    @test e2 isa BoundsError
    @test e2.i == (9, 9)
    @test occursin("at index [9, 9]", sprint(showerror, e2))
end

@testset "MethodError payload (Issue #11374)" begin
    e1 = try
        f11374("a")
    catch e
        e
    end
    @test e1 isa MethodError
    @test e1.f == f11374
    @test e1.args == ("a",)

    # Named numeric fast paths report the real callable and argument.
    e2 = try
        wrap_sqrt11374("s")
    catch e
        e
    end
    @test e2 isa MethodError
    @test e2.f == sqrt
    @test e2.args == ("s",)

    nested = try
        f11374("outer")
    catch
        try
            wrap_sqrt11374("inner")
        catch
        end
        try
            rethrow()
            nothing
        catch rethrown
            rethrown
        end
    end
    @test nested isa MethodError
    @test nested.f == f11374
    @test nested.args == ("outer",)

    # The rendered message is unchanged.
    @test occursin("no method matching f11374(::String)", sprint(showerror, e1))
end

true
