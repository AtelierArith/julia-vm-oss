using Test
using LinearAlgebra

import LinearAlgebra: det, lu

det(A::Array) = 99
lu(A::Array) = :lu_module_dispatch_first_4020

@testset "det/lu module dispatch first #4020" begin
    A = [1 2; 3 4]
    @test LinearAlgebra.det(A) == 99
    @test LinearAlgebra.lu(A) === :lu_module_dispatch_first_4020
    @test LinearAlgebra.det((A,)...) == 99
    @test LinearAlgebra.lu((A,)...) === :lu_module_dispatch_first_4020
end

true
