# Issue #8697: nextest category anchor for the reduced numeric matrix.
#
# The full 3840-cell comparison is run by
# scripts/check_numeric_matrix_reduced.sh because it generates a single sjulia
# probe from the upstream TSV oracle and compares result/status/type/value
# columns against docs/vm/NUMERIC_MATRIX_REDUCED_ALLOWLIST.tsv.

using Test

const NUMERIC_MATRIX_REDUCED_8697_TOTAL_CELLS = 3840
# Issue #8885 (bigfloat-precision, 15 cells) resolved: BigFloat repr now matches
# MPFR's digit count bit-for-bit, so that family was removed from the allowlist.
const NUMERIC_MATRIX_REDUCED_8697_ALLOWLISTED_DIVERGENCES = 1178
const NUMERIC_MATRIX_REDUCED_8697_ALLOWLIST_FAMILIES = 2

@testset "numeric matrix reduced integration metadata (Issue #8697)" begin
    @test NUMERIC_MATRIX_REDUCED_8697_TOTAL_CELLS == 3840
    @test NUMERIC_MATRIX_REDUCED_8697_ALLOWLISTED_DIVERGENCES == 1178
    @test NUMERIC_MATRIX_REDUCED_8697_ALLOWLIST_FAMILIES == 2
end

true
