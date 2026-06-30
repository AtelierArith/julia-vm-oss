using Test

global_binding_4285 = 1
global_binding_reader_4285() = global_binding_4285 + 1

single_nonconst_global_binding_4285 = 1
single_nonconst_global_binding_reader_4285() = single_nonconst_global_binding_4285 + 1

late_global_binding_reader_4285() = late_global_binding_4285 + 1
late_global_binding_4285 = 1

first_value_4285 = global_binding_reader_4285()
global_binding_4285 = 1.5
second_value_4285 = global_binding_reader_4285()

const const_global_binding_4285 = 41
const_global_binding_reader_4285() = const_global_binding_4285 + 1

const const_reassign_guard_4285 = 1
reassign_caught_4285 = false
try
    const_reassign_guard_4285 = 1.5
catch err
    global reassign_caught_4285
    reassign_caught_4285 = true
end

@testset "global binding reassignment avoids stale inferred loads (Issue #4285)" begin
    @test first_value_4285 == 2
    @test second_value_4285 == 2.5
    @test typeof(second_value_4285) === Float64
    @test Base.infer_return_type(global_binding_reader_4285, Tuple{}) === Any
    @test Base.return_types(global_binding_reader_4285, Tuple{})[1] === Any
    @test single_nonconst_global_binding_reader_4285() == 2
    @test Base.infer_return_type(single_nonconst_global_binding_reader_4285, Tuple{}) === Any
    @test Base.return_types(single_nonconst_global_binding_reader_4285, Tuple{})[1] === Any
    @test late_global_binding_reader_4285() == 2
    @test Base.infer_return_type(late_global_binding_reader_4285, Tuple{}) === Any
    @test Base.return_types(late_global_binding_reader_4285, Tuple{})[1] === Any
    @test const_global_binding_reader_4285() == 42
    @test typeof(const_global_binding_reader_4285()) === Int64
    @test Base.infer_return_type(const_global_binding_reader_4285, Tuple{}) === Int64
    @test Base.return_types(const_global_binding_reader_4285, Tuple{})[1] === Int64
    @test reassign_caught_4285 || const_reassign_guard_4285 == 1
    @test const_reassign_guard_4285 == 1
    @test typeof(const_reassign_guard_4285) === Int64
end

true
