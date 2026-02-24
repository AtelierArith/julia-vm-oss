# Test Float64() dispatching to user-defined methods (Issue #634)
# Float64() should dispatch to Pure Julia methods for custom types
#
# Note: Float32 tests are skipped due to limited Float32 support

using Test

# Test case 1: Simple struct with Float64 method
struct MyType end
Float64(::MyType) = 1.0
check1 = Float64(MyType()) == 1.0

# Test case 2: Struct with custom conversion value
struct Temperature
    kelvin::Float64
end
Float64(t::Temperature) = t.kelvin
check2 = Float64(Temperature(300.0)) == 300.0

# Test case 3: Verify builtin Float64 still works for primitives
check3 = Float64(42) == 42.0

# Return true to indicate success
check1 && check2 && check3
