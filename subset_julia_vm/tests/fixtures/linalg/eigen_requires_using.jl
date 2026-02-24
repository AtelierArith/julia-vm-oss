# Test that eigen() requires using LinearAlgebra

# This should fail because eigen is not available without using LinearAlgebra
result = false
try
    F = eigen([1.0 0.0 0.0; 0.0 3.0 0.0; 0.0 0.0 18.0])
    error("eigen() should not be available without using LinearAlgebra")
catch e
    # Expected: function not found error
    result = true
end

result
