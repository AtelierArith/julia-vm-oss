# Type aliases in method signatures obey source order without losing their
# lexical module owner (Issue #11086).

using Test

later_alias_error_11086 = nothing
try
    later_alias_method_11086(x::LaterAlias11086) = x
catch e
    global later_alias_error_11086 = e
end
const LaterAlias11086 = Int64

const EarlierAlias11086 = Int64
earlier_alias_method_11086(x::EarlierAlias11086) = x + 1

module OwnerA11086
const SharedAlias11086 = Int64
const OwnerNestedAlias11086 = Tuple{SharedAlias11086, Float64}
owned_alias_method_11086(x::SharedAlias11086) = :owner_a
nested_owned_alias_method_11086(x::OwnerNestedAlias11086) = :nested_owner_a
end

module OwnerB11086
const SharedAlias11086 = Float64
const OwnerNestedAlias11086 = Tuple{SharedAlias11086, Float64}
owned_alias_method_11086(x::SharedAlias11086) = :owner_b
nested_owned_alias_method_11086(x::OwnerNestedAlias11086) = :nested_owner_b
end

const SharedAlias11086 = String
const OwnerNestedAlias11086 = Tuple{SharedAlias11086, Float64}
main_alias_method_11086(x::SharedAlias11086) = :main
main_nested_alias_method_11086(x::OwnerNestedAlias11086) = :nested_main

qualified_alias_method_11086(x::OwnerA11086.SharedAlias11086) = :qualified_a
qualified_nested_alias_method_11086(x::OwnerA11086.OwnerNestedAlias11086) = :qualified_nested_a

module ExportedAliasOwner11086
export ImportedAlias11086, imported_alias_method_11086
const ImportedAlias11086 = UInt8
imported_alias_method_11086(x::ImportedAlias11086) = Int(x) + 1
end
using .ExportedAliasOwner11086: ImportedAlias11086, imported_alias_method_11086
imported_alias_consumer_11086(x::ImportedAlias11086) = imported_alias_method_11086(x)

const NestedLeafAlias11086 = Int64
const NestedTupleAlias11086 = Tuple{NestedLeafAlias11086, Float64}
nested_alias_method_11086(x::NestedTupleAlias11086) = x[1] + x[2]

module LaterOwner11086
later_alias_error_11086 = nothing
try
    later_alias_method_11086(x::LaterAlias11086) = x
catch e
    global later_alias_error_11086 = e
end
const LaterAlias11086 = Int64
end

@testset "type alias signature source order (Issue #11086)" begin
    @test later_alias_error_11086 isa UndefVarError
    @test LaterOwner11086.later_alias_error_11086 isa UndefVarError
    @test earlier_alias_method_11086(41) == 42

    @test OwnerA11086.owned_alias_method_11086(1) == :owner_a
    @test OwnerB11086.owned_alias_method_11086(1.0) == :owner_b
    @test main_alias_method_11086("main") == :main
    @test OwnerA11086.nested_owned_alias_method_11086((1, 2.0)) == :nested_owner_a
    @test OwnerB11086.nested_owned_alias_method_11086((1.0, 2.0)) == :nested_owner_b
    @test main_nested_alias_method_11086(("main", 2.0)) == :nested_main
    @test qualified_alias_method_11086(1) == :qualified_a
    @test qualified_nested_alias_method_11086((1, 2.0)) == :qualified_nested_a

    @test imported_alias_consumer_11086(UInt8(41)) == 42
    @test nested_alias_method_11086((40, 2.0)) == 42.0
end

true
