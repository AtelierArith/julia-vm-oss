# Issue #7687: eval'ing an Expr(:try) whose catch/finally body mutates an outer
# vector (or stores to a slot) must not leave stale callee frames behind. The
# raising try block previously left its failing call's frame on the VM stack, so
# a side-effecting `StoreSlot` in the catch/finally body wrote out of bounds
# (`StoreSlot: slot out of bounds`).
log = []
result = eval(:(try
    error()
catch
    push!(log, :caught)
    123
finally
    push!(log, :finally)
end))
@assert result == 123
@assert log == [:caught, :finally]

# Slot assignment inside the catch body after a raise.
log2 = []
r2 = eval(:(try error() catch; x = 41; push!(log2, x); x + 1 end))
@assert r2 == 42
@assert log2 == [41]

# Execution must continue cleanly after the eval'd try; stale frames previously
# corrupted later slot stores in the enclosing scope.
acc = 0
for i in 1:3
    global acc += i
end
@assert acc == 6

true
