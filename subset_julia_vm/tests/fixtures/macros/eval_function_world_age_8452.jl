using Test

f_world_age_8452() = 1

function g_existing_world_age_8452()
    @eval f_world_age_8452() = 2
    f_world_age_8452()
end

function g_new_world_age_8452()
    @eval h_world_age_8452() = 1
    h_world_age_8452()
end

@testset "@eval function definitions obey world age (Issue #8452)" begin
    @test g_existing_world_age_8452() == 1
    @test f_world_age_8452() == 2

    @test_throws MethodError g_new_world_age_8452()
    @test Base.invokelatest(h_world_age_8452) == 1
end

true
