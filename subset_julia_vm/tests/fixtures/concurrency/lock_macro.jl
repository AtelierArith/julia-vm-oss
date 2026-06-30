# Test @lock macro
# @lock lk expr expands to: lock(lk); try expr finally unlock(lk) end
# The lock is always released, even when an exception is thrown.

using Test

@testset "@lock acquires and releases the lock" begin
    lk = ReentrantLock()
    @test islocked(lk) == false
    @lock lk begin
        @test islocked(lk) == true
    end
    @test islocked(lk) == false
end

@testset "@lock with assignment in body" begin
    lk = ReentrantLock()
    x = 0
    @lock lk begin
        x = 42
    end
    @test x == 42
    @test islocked(lk) == false
end

@testset "@lock single expression body" begin
    lk = ReentrantLock()
    y = 0
    @lock lk y = 7
    @test y == 7
    @test islocked(lk) == false
end

@testset "@lock returns body value" begin
    lk = ReentrantLock()
    value = @lock lk begin
        123
    end
    @test value == 123
    @test islocked(lk) == false
end

@testset "@lock releases lock on exception" begin
    lk = ReentrantLock()
    threw = false
    try
        @lock lk begin
            error("boom")
        end
    catch e
        threw = true
    end
    @test threw == true
    @test islocked(lk) == false
end

@testset "@lock works with SpinLock" begin
    sl = SpinLock()
    z = 0
    @lock sl begin
        z = 99
    end
    @test z == 99
    @test islocked(sl) == false
end

# @lock used in value position must yield the value of its body's last
# expression, not `nothing`. `@lock` expands to a block whose tail is a
# `try ... finally unlock(...) end`; the value-producing try lowering must
# preserve the (possibly nested-block) body value. Issue #7806.
@testset "@lock in value position yields body value" begin
    lk = ReentrantLock()
    value = (@lock lk begin
        123
    end)
    @test value == 123
    @test islocked(lk) == false

    # single-expression body, value position
    w = (@lock lk 7 + 8)
    @test w == 15
    @test islocked(lk) == false
end

true
