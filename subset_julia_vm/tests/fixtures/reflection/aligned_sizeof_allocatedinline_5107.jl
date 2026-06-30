using Test

# Issue #5107: sizeof(T) for struct types plus Base.aligned_sizeof and
# Base.allocatedinline / Base.datatype_alignment. Builds on the #5100 layout
# (max-of-field-alignments) and #5104 isbits recursion.
#
# NOTE: struct names are multi-character on purpose. Single-uppercase-letter
# names (`P`, `T`, ...) are misclassified as TypeVars by isconcretetype /
# isbitstype (Issue #5252), which is unrelated to this layout work.

struct ASTwoInt5107
    x::Int64
    y::Int64
end

struct ASMixed5107
    a::Int8
    b::Int64
    c::Int8
end

struct ASPacked5107
    a::Int8
    b::Int8
    c::Int8
end

struct ASFloats5107
    x::Float32
    y::Float64
end

struct ASEmpty5107
end

struct ASNested5107
    p::ASTwoInt5107
    z::Int8
end

# Immutable struct with a boxed (String) field: not isbits, but stored inline.
struct ASHasString5107
    s::String
    n::Int64
end

mutable struct ASMutTwo5107
    x::Int64
    y::Int64
end

mutable struct ASMutOne5107
    x::Int64
end

mutable struct ASMutMixed5107
    a::Int8
    b::Int64
    c::Int8
end

# Immutable struct embedding a mutable struct: the mutable field is boxed and
# stored by pointer (8 bytes).
mutable struct ASInner5107
    a::Int64
    b::Int64
end

struct ASOuterMut5107
    m::ASInner5107
    n::Int8
end

@testset "sizeof / aligned_sizeof / allocatedinline (Issue #5107)" begin
    # --- primitive sizeof ---
    @test sizeof(Int) == 8
    @test sizeof(Bool) == 1
    @test sizeof(Float32) == 4
    @test sizeof(Float64) == 8
    @test sizeof(Int8) == 1
    @test sizeof(Char) == 4
    @test sizeof(Nothing) == 0

    # --- isbits struct sizeof (from #5100 layout) ---
    @test sizeof(ASTwoInt5107) == 16
    @test sizeof(ASMixed5107) == 24
    @test sizeof(ASPacked5107) == 3
    @test sizeof(ASFloats5107) == 16
    @test sizeof(ASEmpty5107) == 0
    @test sizeof(ASNested5107) == 24
    @test sizeof(ASHasString5107) == 16

    # --- mutable struct sizeof is the data layout size, not the pointer width ---
    @test sizeof(ASMutTwo5107) == 16
    @test sizeof(ASMutOne5107) == 8
    @test sizeof(ASMutMixed5107) == 24

    # --- a mutable field is stored by pointer inside another struct ---
    @test sizeof(ASInner5107) == 16
    @test sizeof(ASOuterMut5107) == 16
    @test fieldoffset(ASOuterMut5107, 2) == UInt64(8)

    # --- datatype_alignment (max of field alignments) ---
    @test Base.datatype_alignment(Int64) == 8
    @test Base.datatype_alignment(Int8) == 1
    @test Base.datatype_alignment(Float32) == 4
    @test Base.datatype_alignment(ASTwoInt5107) == 8
    @test Base.datatype_alignment(ASPacked5107) == 1
    @test Base.datatype_alignment(ASMixed5107) == 8

    # --- allocatedinline: concrete + immutable (incl. boxed-field immutables) ---
    @test Base.allocatedinline(Int)
    @test Base.allocatedinline(Bool)
    @test Base.allocatedinline(Float64)
    @test Base.allocatedinline(ASTwoInt5107)
    @test Base.allocatedinline(ASMixed5107)
    @test Base.allocatedinline(ASEmpty5107)
    @test Base.allocatedinline(ASNested5107)
    @test Base.allocatedinline(ASHasString5107)
    # mutable structs and variable-size builtins are boxed
    @test !Base.allocatedinline(ASMutTwo5107)
    @test !Base.allocatedinline(ASMutMixed5107)
    @test !Base.allocatedinline(String)
    # abstract types are not inline
    @test !Base.allocatedinline(Number)

    # --- aligned_sizeof rounds sizeof up to alignment for inline types ---
    @test Base.aligned_sizeof(Int) == 8
    @test Base.aligned_sizeof(Bool) == 1
    @test Base.aligned_sizeof(Float32) == 4
    @test Base.aligned_sizeof(Char) == 4
    @test Base.aligned_sizeof(ASTwoInt5107) == 16
    @test Base.aligned_sizeof(ASMixed5107) == 24
    @test Base.aligned_sizeof(ASPacked5107) == 3
    @test Base.aligned_sizeof(ASFloats5107) == 16
    @test Base.aligned_sizeof(ASEmpty5107) == 0
    @test Base.aligned_sizeof(ASNested5107) == 24
    @test Base.aligned_sizeof(ASHasString5107) == 16
    # non-inline (mutable / abstract) types fall back to the pointer width
    @test Base.aligned_sizeof(ASMutTwo5107) == 8
    @test Base.aligned_sizeof(ASMutMixed5107) == 8

    # --- return types ---
    @test typeof(sizeof(ASTwoInt5107)) === Int
    @test typeof(Base.aligned_sizeof(ASTwoInt5107)) === Int
    @test typeof(Base.allocatedinline(ASTwoInt5107)) === Bool
    @test typeof(Base.datatype_alignment(ASTwoInt5107)) === Int
end

true
