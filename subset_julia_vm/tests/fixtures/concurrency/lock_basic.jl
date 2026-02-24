# Test basic lock types and functions

using Test

@testset "ReentrantLock creation" begin
    lk = ReentrantLock()
    @test isa(lk, ReentrantLock)
    @test islocked(lk) == false
end

@testset "ReentrantLock lock and unlock" begin
    lk = ReentrantLock()

    @test islocked(lk) == false
    lock(lk)
    @test islocked(lk) == true
    unlock(lk)
    @test islocked(lk) == false
end

@testset "SpinLock creation" begin
    sl = SpinLock()
    @test isa(sl, SpinLock)
    @test islocked(sl) == false
end

@testset "SpinLock lock and unlock" begin
    sl = SpinLock()

    @test islocked(sl) == false
    lock(sl)
    @test islocked(sl) == true
    unlock(sl)
    @test islocked(sl) == false
end

@testset "Condition creation" begin
    c = Condition()
    @test isa(c, Condition)
end

true
