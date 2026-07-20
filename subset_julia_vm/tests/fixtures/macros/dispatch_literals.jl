# Test macro dispatch with literal types (Float64, String)

macro literal(x::Float64)
    :(100)
end

macro literal(x::String)
    :(200)
end

macro literal(x::Int)
    :(300)
end

macro literal(x)
    :(0)  # fallback
end

r1 = @literal(3.14)     # Float64 → 100
r2 = @literal("hello")  # String → 200
r3 = @literal(42)       # Int → 300

r1 + r2 + r3  # Expected: 600
