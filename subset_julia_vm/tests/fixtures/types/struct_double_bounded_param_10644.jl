# A struct parameter declared with BOTH bounds (`Lo<:T<:Hi`, and the
# mirrored `Hi>:T>:Lo`) parses and binds the lower AND the upper bound,
# matching upstream Julia; instantiation outside either bound raises
# TypeError at type application. Issue #10644.

using Test

struct DB10644{Int8<:T<:Signed}
    x::T
end

struct DBEmpty10644{Int8<:T<:Signed} end

struct DBRev10644{Signed>:T>:Int8} end

abstract type DBAbs10644{Int8<:T<:Signed} end

f10644(x::T) where {Int8<:T<:Signed} = T
g10644(x::T) where Int8<:T<:Signed = T
h10644(x::T) where {Int8<:T<:Signed} = 1

@testset "double-bounded struct parameter Lo<:T<:Hi (Issue #10644)" begin
    # Within both bounds: Int8 satisfies Int8 <: T <: Signed.
    @test DB10644{Int8} isa Type
    @test DBEmpty10644{Int8} isa Type
    @test DBEmpty10644{Signed} isa Type
    @test DBRev10644{Int8} isa Type

    # Constructible within bounds.
    v = DB10644{Int8}(Int8(3))
    @test v.x == Int8(3)

    # Outside the lower bound: Int16 is not >: Int8.
    @test_throws TypeError DBEmpty10644{Int16}
    @test_throws TypeError DBRev10644{Int16}

    # Outside the upper bound: Integer is not <: Signed.
    @test_throws TypeError DBEmpty10644{Integer}

    # Function-signature double bounds (braced and unbraced where forms).
    @test f10644(Int8(1)) == Int8
    @test g10644(Int8(2)) == Int8
    # Dispatch is existential: Int16 matches with T == Signed upstream
    # (a body that does not use T succeeds).
    @test h10644(Int16(1)) == 1
end

true
