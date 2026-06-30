# Issue #5178: jump/back-edge opcodes dispatched at top of the VM loop.
# Exercises every conditional-jump fusion opcode plus JumpIfZero and an
# unconditional Jump (loop back-edge) to guard the prioritized dispatch path.

using Test

# JumpIfLtI64 / JumpIfGeI64 back-edge: `while i < n` counting loop.
function count_up(n)
    i = 0
    while i < n
        i = i + 1
    end
    i
end

# JumpIfZero: `if cond` over a boolean expression.
function classify(x)
    if x > 0
        return 1
    else
        return -1
    end
end

# Compare-jump fusion across the full operator set inside a loop body.
function fused_ops(n)
    total = 0
    i = 1
    while i <= n
        if i == 3
            total = total + 100
        end
        if i != 3
            total = total + 1
        end
        if i > 2
            total = total + 10
        end
        if i >= 4
            total = total + 1000
        end
        i = i + 1
    end
    total
end

# Nested loops to stress the unconditional Jump (Instr::Jump) back-edge.
function nested(n)
    acc = 0
    for a in 1:n
        for b in 1:n
            acc = acc + 1
        end
    end
    acc
end

@testset "Issue #5178 jump dispatch priority" begin
    @test count_up(10) == 10
    @test count_up(0) == 0
    @test classify(5) == 1
    @test classify(-5) == -1
    # i=1: +1; i=2: +1; i=3: +100+10; i=4: +1+10+1000; i=5: +1+10+1000
    @test fused_ops(5) == (1 + 1 + (100 + 10) + (1 + 10 + 1000) + (1 + 10 + 1000))
    @test nested(4) == 16
    @test nested(0) == 0
end

true
