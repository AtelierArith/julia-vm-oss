using Test

# Issue #4285: redefining a const global with a new (same-type) value must
# update functions that read it — the dependent runtime value and its inferred
# return type both track the redefinition, never a stale cached result.
const REDEF_CONST_4285 = 10
redef_const_reader_4285() = REDEF_CONST_4285 + 1
first_const_read_4285 = redef_const_reader_4285()
first_const_infer_4285 = Base.infer_return_type(redef_const_reader_4285, Tuple{})

const REDEF_CONST_4285 = 20
second_const_read_4285 = redef_const_reader_4285()
second_const_infer_4285 = Base.infer_return_type(redef_const_reader_4285, Tuple{})

# A second, independent const reader must keep its own precise inference across
# the unrelated redefinition above (targeted, not wholesale, invalidation).
const OTHER_CONST_4285 = 100
other_const_reader_4285() = OTHER_CONST_4285 + 1
other_const_read_4285 = other_const_reader_4285()
other_const_infer_4285 = Base.infer_return_type(other_const_reader_4285, Tuple{})

@testset "const redefinition updates dependent inference (Issue #4285)" begin
    @test first_const_read_4285 == 11
    @test first_const_infer_4285 === Int64
    @test second_const_read_4285 == 21
    @test second_const_infer_4285 === Int64
    @test redef_const_reader_4285() == 21
    @test typeof(redef_const_reader_4285()) === Int64
    @test other_const_read_4285 == 101
    @test other_const_infer_4285 === Int64
    @test other_const_reader_4285() == 101
end

true
