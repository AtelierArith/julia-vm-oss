using Test

promote_type_value_9914() = promote_type(Int64, Float64)

promote_type_selector_9914(::Type{Float64}) = :float64
promote_type_selector_9914(::Type) = :other
promote_type_dispatch_9914() = promote_type_selector_9914(promote_type(Int64, Float64))

@testset "promote_type inference preserves returned Type{T} value (Issue #9914)" begin
    @test Base.infer_return_type(promote_type_value_9914, Tuple{}) === Type{Float64}
    @test Core.Compiler.return_type(promote_type_value_9914, Tuple{}) === Type{Float64}
    @test promote_type_value_9914() === Float64
    @test promote_type_dispatch_9914() === :float64
end

true
