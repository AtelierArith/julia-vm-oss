# Regression: signbit(Float16/Float32(-0.0)) must see the sign bit (Issue #9529).
# The generic sign-comparison signbit(x) in number.jl compares -0.0 == 0 and
# returns false; the narrow floats need a bit-level method (reinterpret to the
# matching signed-int width, then integer signbit), mirroring upstream
# floatfuncs.jl signbit(x::Float32)=signbit(bitcast(Int32,x)) (Float16 analog).

# negative zero of each width
@assert signbit(Float16(-0.0)) == true
@assert signbit(Float32(-0.0)) == true
@assert signbit(-0.0) == true

# positive zero
@assert signbit(Float16(0.0)) == false
@assert signbit(Float32(0.0)) == false
@assert signbit(0.0) == false

# ordinary negatives / positives
@assert signbit(Float16(-1.5)) == true
@assert signbit(Float32(-1.5)) == true
@assert signbit(Float16(2.0)) == false
@assert signbit(Float32(2.0)) == false

# infinities
@assert signbit(-Inf32) == true
@assert signbit(Inf32) == false
@assert signbit(Float16(-Inf)) == true
@assert signbit(Float16(Inf)) == false

println("ok")
true
