# Test Irrational type for user-defined mathematical constants (Issue #533)
# Tests the AbstractIrrational type and user-defined irrational constants

using Test

# Test 1: AbstractIrrational is a subtype of Real
@test AbstractIrrational <: Real

# Test 2: User-defined irrational constant using AbstractIrrational
struct MyConstant{sym} <: AbstractIrrational end
Float64(::MyConstant{:sqrt2}) = 1.4142135623730951
Float64(::MyConstant{:sqrt3}) = 1.7320508075688772

const sqrt2_const = MyConstant{:sqrt2}()
const sqrt3_const = MyConstant{:sqrt3}()

@test Float64(sqrt2_const) == 1.4142135623730951
@test Float64(sqrt3_const) == 1.7320508075688772

# Test 3: Dispatch through wrapper function (Issue #633)
function get_float_value(x)
    return Float64(x)
end

@test get_float_value(sqrt2_const) == 1.4142135623730951
@test get_float_value(sqrt3_const) == 1.7320508075688772

# Test 4: Base math constants (Float64 values)
@test typeof(pi) == Float64
@test typeof(e) == Float64
@test pi ≈ 3.141592653589793 atol=1e-15
@test e ≈ 2.718281828459045 atol=1e-15

# Return true to indicate success
true
