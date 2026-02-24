# Test: Functions can modify global const arrays
# Issue: #341 - const arrays defined in prelude files cannot be accessed from functions

const _counter = [0]

function increment_counter()
    _counter[1] += 1
    return _counter[1]
end

# Call function multiple times to verify mutations persist
result1 = increment_counter()
result2 = increment_counter()
result3 = increment_counter()

# Expected: 1, 2, 3 (counter is incremented each time)
Float64(result1 + result2 + result3)  # 1 + 2 + 3 = 6
