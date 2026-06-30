using Test

@testset "Base.retry (Issue #8371)" begin
    f = retry(() -> 42)
    @test f() == 42

    attempts = [0]
    eventually_ok = retry(() -> begin
        attempts[1] += 1
        if attempts[1] < 2
            error("transient")
        end
        attempts[1]
    end; delays=[0.0])
    @test eventually_ok() == 2
    @test attempts[1] == 2

    blocked_attempts = [0]
    blocked = retry(() -> begin
        blocked_attempts[1] += 1
        error("blocked")
    end; delays=[0.0], check=(state, err) -> false)
    blocked_caught = false
    try
        blocked()
    catch err
        blocked_caught = err isa ErrorException
    end
    @test blocked_caught
    @test blocked_attempts[1] == 1

    kw_attempts = [0]
    kw_retry = retry((x; scale=1) -> begin
        kw_attempts[1] += 1
        if kw_attempts[1] < 2
            error("kw transient")
        end
        x * scale
    end; delays=[0.0])
    @test kw_retry(21; scale=2) == 42
    @test kw_attempts[1] == 2
end

true
