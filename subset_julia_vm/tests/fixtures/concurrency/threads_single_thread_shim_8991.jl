# Base.Threads single-thread compatibility shim (Issue #8991)

using Test

@testset "Threads single-thread compatibility shim" begin
    @test Threads.nthreads() >= 1
    @test Threads.maxthreadid() >= Threads.nthreads()
    @test 1 <= Threads.threadid() <= Threads.maxthreadid()
    @test Base.Threads.nthreads() == Threads.nthreads()

    acc = zeros(Int, 3)
    Threads.@threads for i in 1:3
        acc[i] = i
    end
    @test acc == [1, 2, 3]

    task = Threads.@spawn begin
        40 + 2
    end
    @test fetch(task) == 42

    atomic = Threads.Atomic{Int}(1)
    @test atomic[] == 1
    @test Threads.atomic_add!(atomic, 2) == 1
    @test atomic[] == 3
    @test Threads.atomic_xchg!(atomic, 9) == 3
    @test atomic[] == 9

    lock = Threads.SpinLock()
    @test Threads.trylock(lock)
    @test Threads.islocked(lock)
    Threads.unlock(lock)
    @test !Threads.islocked(lock)
end

true
