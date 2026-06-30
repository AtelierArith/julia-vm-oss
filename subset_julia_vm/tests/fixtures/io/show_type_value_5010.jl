# Issue #5010: `show`/`repr` displayed Type (DataType) values as
# `DataType()` instead of the type name. Root cause: `base/io.jl`
# had no `show(io::IO, x::Type)` method, so Type values fell through
# to the generic struct fallback `show(io::IO, x)` which printed
# `typeof(x)` = `DataType` followed by a parenthesized (empty) field
# list, yielding `DataType()`. `string(::Type)` already produced the
# correct name (e.g. `"Symbol"`), so the fix routes the 2-arg show
# path through it.
#
# Verified against upstream Julia 1.12:
#   repr(Symbol)          == "Symbol"
#   repr(typeof(:foo))    == "Symbol"
#   repr(Int64)           == "Int64"
#   string(typeof(:foo))  == "Symbol"

using Test

@testset "show(io, ::Type) prints the type name (Issue #5010)" begin
    @test repr(Symbol) == "Symbol"
    @test repr(typeof(:foo)) == "Symbol"
    @test repr(Int64) == "Int64"
    @test repr(Float64) == "Float64"
    @test repr(String) == "String"
    @test repr(Bool) == "Bool"
end

@testset "string(::Type) matches repr(::Type) (Issue #5010)" begin
    @test string(typeof(:foo)) == "Symbol"
    @test string(Int64) == "Int64"
    @test repr(Symbol) == string(Symbol)
    @test repr(typeof(:foo)) == string(typeof(:foo))
end

true
