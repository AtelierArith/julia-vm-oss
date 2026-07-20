# A module-local type alias whose leaf name matches a builtin must stay
# private to its lexical owner: a sibling module's bare annotation resolves
# through its own implicit `using Base`, not the sibling's alias
# (Issue #11452).

using Test

baremodule AliasOwner11452
const BigInt = Int64
end

module Consumer11452
f(x::BigInt) = 1
r = f(BigInt(1))
end

# Control: an exported alias imported with `using` keeps resolving bare.
module ExportedOwner11452
export ImportedAlias11452
const ImportedAlias11452 = UInt8
end
using .ExportedOwner11452: ImportedAlias11452
imported_consumer_11452(x::ImportedAlias11452) = Int(x) + 1

@testset "sibling module alias does not leak bare (Issue #11452)" begin
    @test Consumer11452.r == 1
    @test AliasOwner11452.BigInt === Int64
    @test imported_consumer_11452(UInt8(41)) == 42
end

true
