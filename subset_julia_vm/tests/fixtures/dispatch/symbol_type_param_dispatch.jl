# Test symbol type parameters in parametric structs and Float64 dispatch (Issue #633)
# Float64() should dispatch to user-defined methods for struct types with symbol type parameters

using Test

# Test: Custom type with symbol type parameter (using AbstractIrrational from Base)
struct MyIrrational{sym} <: AbstractIrrational end
Float64(::MyIrrational{:tau}) = 6.283185307179586  # 2*pi
Float64(::MyIrrational{:sqrt2}) = 1.4142135623730951

const tau_val = MyIrrational{:tau}()
const sqrt2_val = MyIrrational{:sqrt2}()

# Test direct calls work
@test Float64(tau_val) == 6.283185307179586
@test Float64(sqrt2_val) == 1.4142135623730951

# Test calls through wrapper function with Any-typed parameter
function convert_to_float(x)
    return Float64(x)
end

@test convert_to_float(tau_val) == 6.283185307179586
@test convert_to_float(sqrt2_val) == 1.4142135623730951

# Test builtin Float64 still works for numeric types
@test Float64(42) == 42.0
@test Float64(3.14f0) ≈ 3.14 atol=1e-6

# Test Base pi constant (Float64)
@test pi == 3.141592653589793
@test 2.0 * pi == 6.283185307179586

# Return true to indicate success
true
