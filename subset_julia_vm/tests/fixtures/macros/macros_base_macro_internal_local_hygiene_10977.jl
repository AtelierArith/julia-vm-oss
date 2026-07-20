# Issue #10977: Base macros' OWN quote-introduced locals must be
# gensym-renamed (hygienic) so they never clobber same-named caller variables,
# while caller-spliced names (the `@time grid = ...` case, Issue #9619) keep
# resolving in the caller's scope.
#
# The @time/@elapsed/@timev/@timed/@showtime family prints timing lines whose
# numbers differ between runtimes, so every assertion is on VARIABLE VALUES
# via @test — never on timing output text. All macro calls run BEFORE the
# @testsets so the nondeterministic timing lines never interleave with the
# Test Summary blocks (scripts/fixture_julia_parity.sh parses those blocks).

using Test

# ---- local-scope collisions: caller locals named after macro internals ----

function elapsed_with_local_t0()
    t0 = "caller value"
    e = @elapsed(1 + 1)
    (t0, e)
end

function time_with_local_internals()
    t0 = "caller t0"
    result = "caller result"
    elapsed_ns = "caller elapsed_ns"
    elapsed_s = "caller elapsed_s"
    r = @time(1 + 1)
    (r, t0, result, elapsed_ns, elapsed_s)
end

function timev_with_local_internals()
    t0 = 1
    result = 2
    elapsed_ns = 3
    elapsed_s = 4
    r = @timev(10 * 10)
    (r, t0, result, elapsed_ns, elapsed_s)
end

function timev_msg_with_local_internals()
    msg_val = "caller msg_val"
    t0 = "caller t0"
    r = @timev("timing label", 3 + 4)
    (r, msg_val, t0)
end

function timed_with_local_internals()
    t0 = "caller t0"
    result = "caller result"
    elapsed_s = "caller elapsed_s"
    stats = @timed(5 + 5)
    (stats, t0, result, elapsed_s)
end

function showtime_with_local_internals()
    t0 = "caller t0"
    result = "caller result"
    r = @showtime(2 + 3)
    (r, t0, result)
end

function alloc_macros_evaluate()
    x = 0
    bytes = @allocated(x = 41 + 1)
    x_after_allocated = x
    n = @allocations(x = x + 1)
    (bytes, x_after_allocated, n, x)
end

function lock_with_local_temp()
    temp = 42
    lk = ReentrantLock()
    r = @lock lk 1 + 1
    (r, temp)
end

elapsed_res = elapsed_with_local_t0()
time_res = time_with_local_internals()
timev_res = timev_with_local_internals()
timev_msg_res = timev_msg_with_local_internals()
timed_res = timed_with_local_internals()
showtime_res = showtime_with_local_internals()
alloc_res = alloc_macros_evaluate()
lock_res = lock_with_local_temp()
shown = @show(1 + 2)

# ---- caller-spliced assignment stays caller-visible (Issue #9619) ----
@time grid = fill(0.0, 2)

# ---- global-scope collisions: the exact Issue #10977 MWE shape ----
t0 = "caller value"
result = "caller result"
elapsed_s = "caller elapsed_s"
e_top = @elapsed(1 + 1)
r_top = @time(sum(1:10))

@testset "@elapsed does not clobber caller t0 (Issue #10977)" begin
    @test elapsed_res[1] == "caller value"
    @test elapsed_res[2] isa Float64
    @test elapsed_res[2] >= 0.0
end

@testset "@time does not clobber caller locals" begin
    @test time_res == (2, "caller t0", "caller result", "caller elapsed_ns", "caller elapsed_s")
end

@testset "@timev does not clobber caller locals" begin
    @test timev_res == (100, 1, 2, 3, 4)
end

@testset "@timev with message does not clobber caller locals" begin
    @test timev_msg_res == (7, "caller msg_val", "caller t0")
end

@testset "@timed does not clobber caller locals" begin
    stats = timed_res[1]
    @test stats.value == 10
    @test stats.time >= 0.0
    @test timed_res[2] == "caller t0"
    @test timed_res[3] == "caller result"
    @test timed_res[4] == "caller elapsed_s"
end

@testset "@showtime does not clobber caller locals" begin
    @test showtime_res == (5, "caller t0", "caller result")
end

@testset "@allocated / @allocations still evaluate the expression" begin
    @test alloc_res[1] isa Integer
    @test alloc_res[2] == 42
    @test alloc_res[3] isa Integer
    @test alloc_res[4] == 43
end

@testset "@show does not leak internals and returns the value" begin
    @test shown == 3
end

@testset "@lock does not clobber caller temp" begin
    @test lock_res == (2, 42)
end

@testset "@time caller-spliced assignment stays caller-visible (Issue #9619)" begin
    @test grid == [0.0, 0.0]
end

@testset "top-level globals named t0/result/elapsed_s survive (MWE)" begin
    @test t0 == "caller value"
    @test result == "caller result"
    @test elapsed_s == "caller elapsed_s"
    @test e_top isa Float64
    @test r_top == 55
end

true
