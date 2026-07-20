# Inner-constructor `new(...)` facts as first-class PartialStruct (Issue #8739).
#
# Before #8739 these facts lived in the ConstructorPartial side cache and the
# TypeEnv partial_structs side table, which could serve `b = Ctor(x); b.v`
# and `getfield(Ctor(x), :v)` locally but could NOT follow the value through
# argument binding into a helper. With the side cache retired, the ctor
# body's `new(...)` surfaces a `LatticeType::PartialStruct` through the
# regular return-type cache, so all shapes below infer precisely.
#
# NOTE: probe functions take an (untyped) parameter on purpose — reflection
# re-infers a body only when a parameter is untyped; zero-parameter functions
# report the main-pipeline snapshot instead (see partial_struct_lattice_8544.jl).
using Test

# Long-form inner constructor over an untyped field: all precision below
# comes from the `new(x + 1)` argument fact, not the declaration.
struct PSInnerL8739
    v
    function PSInnerL8739(x)
        new(x + 1)
    end
end

# Short-form inner constructor.
struct PSInnerS8739
    w
    PSInnerS8739(x) = new(2 * x)
end

# Inner constructor building a NESTED immutable through the default ctor of
# another struct: the `new(...)` argument fact is itself a PartialStruct.
struct PSLeaf8739
    n
end
struct PSTree8739
    leaf
    function PSTree8739(x)
        new(PSLeaf8739(x + 1))
    end
end

ps39_read_v(b) = b.v * 2
ps39_read_w(b) = b.w * 2

# 1. The fact crosses a helper boundary via argument binding (this was the
#    shape the retired side cache could NOT serve: it inferred Any).
ps39_helper(n) = ps39_read_v(PSInnerL8739(n))
ps39_helper_short(n) = ps39_read_w(PSInnerS8739(n))

# 2. Local binding + dot access (previously served by the env side table).
function ps39_local(n)
    b = PSInnerL8739(n)
    b.v * 2
end

# 3. getfield on the ctor call, by name and by constant index (previously
#    served by the expression-shaped side walk).
ps39_getfield_name(n) = getfield(PSInnerL8739(n), :v)
ps39_getfield_index(n) = getfield(PSInnerL8739(n), 1)

# 4. Interprocedural: a helper RETURNS the inner-ctor result; the fact rides
#    the regular return-type cache into the caller.
ps39_make(n) = PSInnerL8739(n)
ps39_via_return(n) = ps39_make(n).v * 2

# 5. Nested chain through an inner ctor whose `new` argument is itself a
#    freshly constructed immutable.
ps39_tree_leaf_n(n) = PSTree8739(n).leaf.n * 2

@testset "inner-ctor new() PartialStruct inference (Issue #8739)" begin
    @test Base.infer_return_type(ps39_helper, Tuple{Int}) === Int64
    @test Base.infer_return_type(ps39_helper_short, Tuple{Int}) === Int64
    @test Base.infer_return_type(ps39_local, Tuple{Int}) === Int64
    @test Base.infer_return_type(ps39_getfield_name, Tuple{Int}) === Int64
    @test Base.infer_return_type(ps39_getfield_index, Tuple{Int}) === Int64
    @test Base.infer_return_type(ps39_via_return, Tuple{Int}) === Int64
    @test Base.infer_return_type(ps39_tree_leaf_n, Tuple{Int}) === Int64
end

@testset "inner-ctor new() PartialStruct behavior (Issue #8739)" begin
    @test ps39_helper(20) == 42
    @test ps39_helper_short(10) == 40
    @test ps39_local(20) == 42
    @test ps39_getfield_name(20) == 21
    @test ps39_getfield_index(20) == 21
    @test ps39_via_return(20) == 42
    @test ps39_tree_leaf_n(20) == 42
end

true
