# Test macro dispatch specificity: more specific types win
# Symbol/Int/Float/String are more specific than Expr, which is more specific than Any

macro specific(x::Symbol)
    :(10)
end

macro specific(x::Expr)
    :(20)
end

macro specific(x)  # Any
    :(30)
end

r1 = @specific(foo)    # Symbol → 10
r2 = @specific(1 + 2)  # Expr → 20
r3 = @specific(42)     # Int (no Int handler, falls to Any) → 30

r1 + r2 + r3  # Expected: 60
