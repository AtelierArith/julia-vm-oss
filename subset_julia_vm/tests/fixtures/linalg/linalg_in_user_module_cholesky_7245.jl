# Issue #7245: `using LinearAlgebra` inside a user-defined module named `D1`.
# LinearAlgebra also has methods with parameters named `D1`/`D2` (e.g. the
# `Diagonal * Diagonal` method does `D1.diag[i] * D2.diag[i]`), so a user
# module literally named `D1` triggered the same mis-resolution as `D`.

using Test
using LinearAlgebra

module D1
using LinearAlgebra
dchol(S) = cholesky(S).U
export dchol
end
using .D1

@testset "cholesky usable from a user module named D1 (Issue #7245)" begin
    @test D1.dchol([4.0 0.0; 0.0 9.0]) == [2.0 0.0; 0.0 3.0]
end

true  # Test passed
