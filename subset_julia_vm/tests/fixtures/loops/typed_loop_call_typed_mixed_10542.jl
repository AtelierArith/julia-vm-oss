# Issue #10542: direct-call typed-loop inlining of MIXED-TYPE callees.
#
# The typed-loop recognizer's direct-call arms (`Instr::Call`/`CallResolved`/
# `CallInbounds` -> typed_loop_f64_call_op, and `CallResolvedI64Slots` ->
# typed_loop_i64_call_op) used to predecode the callee ONLY with the PURE
# single-type scalar decoders (try_predecode_f64_function /
# try_predecode_i64_function). A callee whose declared params/return are a
# single type (all-F64 or all-I64) but whose BODY mixes types (e.g. an
# F64-param helper with an I64 loop counter) failed that pure predecode, and
# the whole caller loop was rejected -- even though the exact same logic
# reached through an *untyped* helper (CallSpecializeF64Slots, Issue #10491)
# DID inline natively. This fixture pins:
#   1. F64-shaped callee with an internal I64 counter now inlines via
#      TypedLoopOp::CallTypedF64Function and matches upstream Julia.
#   2. I64-shaped callee with an internal F64 local now inlines via
#      TypedLoopOp::CallTypedI64Function and matches upstream Julia.
#   3. Argument order/binding is exercised with a non-commutative body
#      (`x - y`, evaluated both ways) so a swapped-argument mis-binding
#      would change the result.
#   4. A callee with genuinely MIXED declared param types (one Int64 param,
#      one Float64 param) called directly from inside a typed loop still
#      computes correctly. The typed-loop recognizer structurally cannot fuse
#      a mixed-type ARGUMENT run into either direct-call slots form (the
#      peephole fusion pass only fuses homogeneous-typed argument runs), so
#      this call is REJECTED from typed-loop inlining -- not mis-bound -- and
#      stays on the interpreter frame path.

using Test

# --- (1) F64-shaped params, internal I64 loop counter ---------------------
function fstep_mixed_10542(x::Float64, y::Float64)::Float64
    r = x - y
    k = 0
    while k < 4
        r = r + y
        r = r * 0.5
        k = k + 1
    end
    r
end

function scan_f64_10542(N::Int64)::Int64
    cnt = 0
    x = 0.0
    a = 1
    while a <= N
        x = x + 1.0
        y = 0.0
        b = 1
        while b <= N
            y = y + 1.0
            if fstep_mixed_10542(x, y) > 0.5
                cnt = cnt + 1
            end
            b = b + 1
        end
        a = a + 1
    end
    cnt
end

# --- (2) I64-shaped params, internal F64 local -----------------------------
function istep_mixed_10542(a::Int64, b::Int64)::Int64
    r = a - b
    t = 0.0
    k = 0
    while k < 4
        t = t + Float64(b)
        t = t * 0.5
        k = k + 1
    end
    r + Int64(round(t))
end

function scan_i64_10542(N::Int64)::Int64
    cnt = 0
    a = 1
    while a <= N
        b = 1
        while b <= N
            if istep_mixed_10542(a, b) > 0
                cnt = cnt + 1
            end
            b = b + 1
        end
        a = a + 1
    end
    cnt
end

# --- (3) Argument-order sensitivity: non-commutative body -----------------
function order_sensitive_10542(x::Float64, y::Float64)::Float64
    r = x
    k = 0
    while k < 3
        r = r - y
        k = k + 1
    end
    r
end

# --- (4) genuinely mixed declared param types: structurally cannot fuse into
# either direct-call slots form (peephole leaves mixed I64/F64 argument runs
# unfused), so this stays on the interpreter frame path -- still correct.
function mixed_declared_10542(a::Int64, b::Float64)::Float64
    Float64(a) + b
end

function scan_mixed_declared_10542(N::Int64)::Float64
    s = 0.0
    i = 1
    while i <= N
        s = s + mixed_declared_10542(i, 0.5)
        i = i + 1
    end
    s
end

@testset "typed-loop direct-call mixed-type callee inline (Issue #10542)" begin
    @test scan_f64_10542(50) == 2500
    @test scan_i64_10542(20) == 388
    @test order_sensitive_10542(10.0, 3.0) == 1.0
    @test order_sensitive_10542(3.0, 10.0) == -27.0
    @test scan_mixed_declared_10542(10) == 60.0
end

true
