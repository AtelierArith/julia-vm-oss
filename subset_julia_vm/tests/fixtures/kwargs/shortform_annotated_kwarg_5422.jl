# Issue #5422: a short-form method definition (`f(...) = expr`) with an
# ANNOTATED keyword argument (`n::Int = 0`) previously failed to lower with
# `Undefined variable: n` — the short-form kwarg parser only handled a plain
# `Identifier` name and dropped the keyword parameter when its name was a typed
# expression. The long-form definition already worked; this brings short-form to
# parity. Verified against upstream Julia 1.12.
#
# (Array-typed defaults like `v::Vector{Int} = [1,2]` hit a separate, pre-existing
# kwarg-slot-typing bug present in long-form too — out of scope here, see #5425.)

using Test

# Single short-form method with an annotated keyword argument.
single_int(; n::Int = 0) = n
single_float(x; m::Float64 = 1.0) = x + m

# Original #5422 report: two short-form keyword-only methods sharing a kwarg
# name, one annotated and one not.
unann(; n = 0) = n
ann(; n::Int = 0) = n

@testset "short-form annotated keyword argument (#5422)" begin
    @testset "single annotated kwarg" begin
        @test single_int(n = 2) == 2
        @test typeof(single_int(n = 2)) === Int64
        @test single_int() == 0
        @test single_float(10, m = 2.5) == 12.5
        @test single_float(10) == 11.0
    end

    @testset "two methods sharing a kwarg name (annotated + unannotated)" begin
        @test unann(n = 1) == 1
        @test ann(n = 7) == 7
        @test unann() == 0
        @test ann() == 0
    end
end

true
