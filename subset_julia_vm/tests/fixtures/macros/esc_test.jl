# Test esc for macro hygiene
# esc(expr) escapes an expression to avoid capture by macro scope

# Test: esc with a symbol should work without error
sym = :x
escaped = esc(sym)

# Test: esc with a quoted expression
expr = :(1 + 2)
escaped2 = esc(expr)

# If we get here without errors, esc is working
# Return success
Float64(42)
