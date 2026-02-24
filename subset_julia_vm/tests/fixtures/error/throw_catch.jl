using Test

function catch_domain_error()
    try
        throw(DomainError(0 - 1, "negative"))
        return 0
    catch e
        return 42
    end
end

function try_success()
    try
        return 100
    catch e
        return 0
    end
end

@testset "basic try/catch with errors" begin
    @test catch_domain_error() == 42
    @test try_success() == 100
end

true
