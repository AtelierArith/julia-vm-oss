# A DomainError raised by a VM-internal numeric op (sqrt of a negative real)
# exposes upstream's `.val` — the actual out-of-domain value — instead of a
# `nothing` placeholder, matching the MethodError/BoundsError payload work
# (Issue #11399, tech-debt continuation of #11374). User-thrown DomainErrors
# keep their explicit val unchanged.
using Test

@testset "VM-raised sqrt DomainError carries .val (Issue #11399)" begin
    e1 = try
        sqrt(-1.0)
    catch e
        e
    end
    @test e1 isa DomainError
    @test e1.val === -1.0
    @test occursin("negative real argument", e1.msg)

    e2 = try
        sqrt(-4.0)
    catch e
        e
    end
    @test e2 isa DomainError
    @test e2.val === -4.0

    # BigFloat operand keeps its own type in .val.
    e3 = try
        sqrt(big(-9.0))
    catch e
        e
    end
    @test e3 isa DomainError
    @test e3.val == big(-9.0)
    @test e3.val isa BigFloat

    nested = try
        sqrt(-16.0)
    catch
        try
            sqrt(-25.0)
        catch
        end
        try
            rethrow()
            nothing
        catch rethrown
            rethrown
        end
    end
    @test nested isa DomainError
    @test nested.val === -16.0
end

@testset "user-thrown DomainError keeps its explicit val (Issue #11399)" begin
    e = try
        throw(DomainError(-2.5, "custom"))
    catch err
        err
    end
    @test e isa DomainError
    @test e.val === -2.5
    @test e.msg == "custom"

    # DomainError(val) one-arg form.
    e2 = try
        throw(DomainError(42))
    catch err
        err
    end
    @test e2.val === 42
end

true
