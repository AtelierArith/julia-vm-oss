using Test

const GENERATED_SIGNATURE_COUNTER_5936 = Int64[]

@generated function generated_signature_cache_5936(x)
    push!(GENERATED_SIGNATURE_COUNTER_5936, 1)
    return :(x)
end

@testset "generated signature cache (Issue #5936)" begin
    @test generated_signature_cache_5936(1) == 1
    @test generated_signature_cache_5936(2) == 2
    @test generated_signature_cache_5936(3.0) == 3.0
    @test length(GENERATED_SIGNATURE_COUNTER_5936) == 2
end

generated_signature_cache_5936(4) == 4 &&
    generated_signature_cache_5936(5.0) == 5.0 &&
    length(GENERATED_SIGNATURE_COUNTER_5936) == 2
