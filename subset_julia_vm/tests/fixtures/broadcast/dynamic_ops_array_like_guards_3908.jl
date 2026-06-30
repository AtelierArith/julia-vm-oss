# Regression: dynamic_ops/mod.rs Array-like guard refactor (Issue #3908)
# Routes the dynamic_add / dynamic_sub / dynamic_div paths through an Any-typed
# boundary so the per-arm guards classify array-like operands via the central
# is_array_like_value helper instead of an explicit Value::Array pattern.

function force_any(x)::Any
    x
end

function check_array_array()
    a = [1.0, 2.0, 3.0]
    b = [4.0, 5.0, 6.0]
    s = force_any(a) + force_any(b)
    d = force_any(a) - force_any(b)
    s[1] == 5.0 && s[3] == 9.0 && d[2] == -3.0 && d[3] == -3.0
end

function check_array_scalar()
    a = [2.0, 4.0, 8.0]
    q = force_any(a) / force_any(2.0)
    r = force_any(16.0) ./ force_any(a)
    q[1] == 1.0 && q[3] == 4.0 && r[1] == 8.0 && r[3] == 2.0
end

check_array_array() && check_array_scalar()
