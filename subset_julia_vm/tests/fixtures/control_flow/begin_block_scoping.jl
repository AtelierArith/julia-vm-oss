# Test: begin...end blocks share the enclosing scope (Issue #2868)
# Variables declared inside begin...end are visible after the block.

# Basic begin...end with multiple statements
x = begin
    a = 1
    b = 2
    a + b
end

# Variables declared inside are visible in outer scope
a_visible = a  # should be 1
b_visible = b  # should be 2

# Nested begin...end
y = begin
    c = 10
    d = begin
        e = 5
        c + e
    end
    d * 2
end

x == 3 && a_visible == 1 && b_visible == 2 && y == 30
