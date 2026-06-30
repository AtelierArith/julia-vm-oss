using Test

struct A4270
    n::Int64
end

struct B4270
    s::String
end

struct Box4270
    value::Union{Int64,Nothing}
end

function narrowed_alias_field_4270(x::Union{A4270,B4270})
    if x isa A4270
        y = x
        return y.n
    end
    return 0
end

field_get_4270(x::Box4270) = getfield(x, :value)

@testset "narrowed field access propagates through local aliases" begin
    @test Base.infer_return_type(narrowed_alias_field_4270, Tuple{Union{A4270,B4270}}) === Int64
    @test Core.Compiler.return_type(narrowed_alias_field_4270, Tuple{Union{A4270,B4270}}) === Int64
    @test narrowed_alias_field_4270(A4270(7)) == 7
    @test narrowed_alias_field_4270(B4270("z")) == 0
end

@testset "getfield preserves declared union field types" begin
    @test Base.infer_return_type(field_get_4270, Tuple{Box4270}) == Union{Nothing,Int64}
    @test Core.Compiler.return_type(field_get_4270, Tuple{Box4270}) == Union{Nothing,Int64}
    @test field_get_4270(Box4270(7)) == 7
    @test field_get_4270(Box4270(nothing)) === nothing
end

true
