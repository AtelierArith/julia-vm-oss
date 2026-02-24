# Test dynamic typing across reassignments (top-level code)
# Bug: x = 2 after x = 1.0 should have typeof(x) == Int64, not Float64
# This tests Julia's dynamic typing at the top-level using isa()

f(x) = 2x + 1

x = 1
t1_ok = isa(x, Int64)               # true

x = 1.0
t2_ok = isa(x, Float64)             # true

x = 2
t3_ok = isa(x, Int64)               # Should be true (was incorrectly Float64)

# Also test function return type preserves input type
fx1 = isa(f(1), Int64)              # true
fx2 = isa(f(1.0), Float64)          # true
fx3 = isa(f(2), Int64)              # Should be true

# Return true if all tests pass
t1_ok && t2_ok && t3_ok && fx1 && fx2 && fx3
