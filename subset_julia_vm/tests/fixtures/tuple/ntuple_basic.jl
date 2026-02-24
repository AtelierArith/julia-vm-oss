# Test ntuple function - basic functionality
# ntuple(f, n) creates a tuple by calling f(i) for i in 1:n

# Helper function
id(x) = x

# Wrap in function to avoid scoping issues
function test_ntuple()
    t = ntuple(id, 5)
    total = 0
    for x in t
        total = total + x
    end
    total
end

test_ntuple()  # Return value: 1+2+3+4+5 = 15
