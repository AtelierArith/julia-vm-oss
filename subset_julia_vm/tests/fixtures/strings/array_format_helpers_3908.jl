# Issue #3908 - Display/format boundary regression coverage.
#
# The Rust formatting module routes `value_to_julia_code` (the surface that
# string interpolation lowers to) through the new file-local
# `legacy_array_value_ref` helper, while the exhaustive `format_value_slow`
# (`print`) and `value_to_string` (`string(x)`) arms continue to call the
# shared `format_array_value` helper directly. This fixture pins the
# `print` / `string` / "$x" output for Int64, Float64, empty, String, and 2D
# arrays so the helper routing stays byte-identical to upstream Julia 1.12.

# 1D Int64 vector via `string`
@assert string([1, 2, 3]) == "[1, 2, 3]"

# 1D Int64 vector via interpolation (exercises value_to_julia_code path)
@assert "$([1, 2, 3])" == "[1, 2, 3]"

# 1D Float64 vector via `string` and interpolation
@assert string([1.0, 2.0]) == "[1.0, 2.0]"
@assert "$([1.0, 2.0])" == "[1.0, 2.0]"

# Empty typed vector — `T[]` form
@assert string(Int64[]) == "Int64[]"
@assert "$(Int64[])" == "Int64[]"

# 1D String vector
let v = ["a", "b"]
    @assert string(v) == "[\"a\", \"b\"]"
    @assert "$v" == "[\"a\", \"b\"]"
end

# 2D matrix compact form
@assert string([1 2; 3 4]) == "[1 2; 3 4]"
@assert "$([1 2; 3 4])" == "[1 2; 3 4]"

true
