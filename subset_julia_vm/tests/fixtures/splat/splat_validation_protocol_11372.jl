using Test

positional_values_11372(values...) = values
keyword_values_11372(; options...) = options
combined_values_11372(values...; options...) = (values, options)
combined_sum_11372(values...; options...) = sum(values) + options[:a].value
dynamic_keys_11400(value) = keys(value)

struct MultiIter11372
    values::Tuple
end

function Base.iterate(iter::MultiIter11372)
    isempty(iter.values) && return nothing
    return (iter.values[1], 2)
end

function Base.iterate(iter::MultiIter11372, state::Int)
    state > length(iter.values) && return nothing
    return (iter.values[state], state + 1)
end

struct ScalarStepIter11372 end
Base.iterate(::ScalarStepIter11372) = 1

struct OneFieldStepIter11372 end
Base.iterate(::OneFieldStepIter11372) = (1,)

struct ExtraTupleStepIter11372 end
Base.iterate(::ExtraTupleStepIter11372) = (11, nothing, 99)
Base.iterate(::ExtraTupleStepIter11372, ::Nothing) = nothing

struct NamedStepIter11372 end
Base.iterate(::NamedStepIter11372) = (value = 12, state = nothing, extra = 99)
Base.iterate(::NamedStepIter11372, ::Nothing) = nothing

struct StepResult11372
    value::Int
    state::Nothing
    extra::Int
end

struct StructStepIter11372 end
Base.iterate(::StructStepIter11372) = StepResult11372(13, nothing, 99)
Base.iterate(::StructStepIter11372, ::Nothing) = nothing

mutable struct MutableStepResult11372
    value::Int
    state::Nothing
    extra::Int
end

struct MutableStructStepIter11372 end
Base.iterate(::MutableStructStepIter11372) = MutableStepResult11372(14, nothing, 99)
Base.iterate(::MutableStructStepIter11372, ::Nothing) = nothing

struct ThrowingSplatIter11372 end
Base.iterate(::ThrowingSplatIter11372) = error("iterate boom 11372")

struct CatchingInnerIter11372 end
function Base.iterate(::CatchingInnerIter11372)
    try
        error("inner iterate catch 11372")
    catch err
        @test err isa ErrorException
        return (17, nothing)
    end
end
Base.iterate(::CatchingInnerIter11372, ::Nothing) = nothing

struct SymbolStateOnlyIter11372 end
Base.iterate(::SymbolStateOnlyIter11372) = (18, :done)

struct RangeStepResultIter11372 end
Base.iterate(::RangeStepResultIter11372) = 10:20
Base.iterate(::RangeStepResultIter11372, ::Int) = nothing

const STEP_RANGE_LEN_RESULT_11372 = range(1.0, 2.0; length = 4)
struct StepRangeLenResultIter11372 end
Base.iterate(::StepRangeLenResultIter11372) = STEP_RANGE_LEN_RESULT_11372
Base.iterate(::StepRangeLenResultIter11372, ::Base.TwicePrecision) = nothing

struct MalformedFirstOuter11372 end
Base.iterate(::MalformedFirstOuter11372) = ((:a,), 2)
Base.iterate(::MalformedFirstOuter11372, ::Int) = error("outer second step must not run 11372")

struct ThirdStepThrowEntry11372 end
Base.iterate(::ThirdStepThrowEntry11372) = (:a, 2)
Base.iterate(::ThirdStepThrowEntry11372, state::Int) =
    state == 2 ? (1, 3) : error("entry third step must not run 11372")

struct NonSymbolThenThrowEntry11372 end
Base.indexed_iterate(::NonSymbolThenThrowEntry11372, ::Int) = (1, 2)
Base.indexed_iterate(::NonSymbolThenThrowEntry11372, ::Int, ::Int) =
    error("entry second field wins 11372")

mutable struct Box11372
    value::Int
end

mutable struct MutableCollectionGc11372
    values::Tuple
end

function Base.iterate(iter::MutableCollectionGc11372)
    GC.gc()
    return (iter.values[1], 2)
end

function Base.iterate(iter::MutableCollectionGc11372, state::Int)
    GC.gc()
    state > length(iter.values) && return nothing
    return (iter.values[state], state + 1)
end

struct YieldBoxThenGc11372 end
Base.iterate(::YieldBoxThenGc11372) = (Box11372(41), 2)
function Base.iterate(::YieldBoxThenGc11372, ::Int)
    GC.gc()
    return nothing
end

struct GcOnce11372 end
function Base.iterate(::GcOnce11372)
    GC.gc()
    return (1, nothing)
end
Base.iterate(::GcOnce11372, ::Nothing) = nothing

struct KwYieldBoxThenGc11372 end
Base.iterate(::KwYieldBoxThenGc11372) = (:a => Box11372(43), 2)
function Base.iterate(::KwYieldBoxThenGc11372, ::Int)
    GC.gc()
    return nothing
end

mutable struct IndexedOnlyGcEntry11372
    key::Symbol
    value::Box11372
end

function Base.indexed_iterate(entry::IndexedOnlyGcEntry11372, ::Int)
    GC.gc()
    return (entry.key, 2)
end

Base.indexed_iterate(entry::IndexedOnlyGcEntry11372, ::Int, ::Int) = (entry.value, 3)

struct OneFieldSecondEntry11372 end
Base.indexed_iterate(::OneFieldSecondEntry11372, ::Int) = (:one, 2)
Base.indexed_iterate(::OneFieldSecondEntry11372, ::Int, ::Int) = (9,)

struct NonSymbolOneFieldSecondEntry11372 end
Base.indexed_iterate(::NonSymbolOneFieldSecondEntry11372, ::Int) = (1, 2)
Base.indexed_iterate(::NonSymbolOneFieldSecondEntry11372, ::Int, ::Int) = (9,)

mutable struct MutableCallable11372
    box::Box11372
end
(callable::MutableCallable11372)(value::Int) = callable.box.value + value

function catches_iterate_in_calling_frame_11372()
    try
        positional_values_11372(ThrowingSplatIter11372()...)
        return false
    catch err
        return err isa ErrorException && occursin("iterate boom 11372", sprint(showerror, err))
    end
end

raise_iterate_in_descendant_11372() = positional_values_11372(ThrowingSplatIter11372()...)

function catches_iterate_in_ancestor_frame_11372()
    try
        raise_iterate_in_descendant_11372()
        return false
    catch err
        return err isa ErrorException && occursin("iterate boom 11372", sprint(showerror, err))
    end
end

function invalid_keyword_precedes_invalid_positional_11372()
    try
        combined_values_11372(nothing...; 2...)
        return false
    catch err
        return err isa BoundsError
    end
end

function valid_keyword_precedes_invalid_positional_11372()
    try
        combined_values_11372(nothing...; (a = 1,) ...)
        return false
    catch err
        return err isa MethodError && occursin("iterate", sprint(showerror, err))
    end
end

const EVALUATION_LOG_11372 = Symbol[]

function logged_invalid_positional_11372()
    push!(EVALUATION_LOG_11372, :positional)
    return nothing
end

function logged_invalid_keyword_11372()
    push!(EVALUATION_LOG_11372, :keyword)
    return 2
end

function evaluation_precedes_keyword_then_positional_validation_11372()
    empty!(EVALUATION_LOG_11372)
    try
        combined_values_11372(
            logged_invalid_positional_11372()...;
            logged_invalid_keyword_11372()...,
        )
        return false
    catch err
        return EVALUATION_LOG_11372 == [:positional, :keyword] && err isa BoundsError
    end
end

function seed_dead_box_before_gc_11372()
    dead_box = Box11372(-1)
    @test dead_box.value == -1
    return nothing
end

function malformed_entry_precedes_next_outer_step_11372()
    try
        keyword_values_11372(; MalformedFirstOuter11372()...)
        return false
    catch err
        return err isa BoundsError
    end
end

function second_entry_field_precedes_key_typeassert_11372()
    try
        keyword_values_11372(; (NonSymbolThenThrowEntry11372(),)...)
        return false
    catch err
        return err isa ErrorException &&
               occursin("entry second field wins 11372", sprint(showerror, err))
    end
end

@testset "splat validation protocol 11372" begin
@test positional_values_11372(MultiIter11372((7, 8, 9))...) == (7, 8, 9)
@test positional_values_11372((1 => 2)...) == (1, 2)
@test positional_values_11372((3:5)...) == (3, 4, 5)
@test positional_values_11372([6, 7]...) == (6, 7)
@test dynamic_keys_11400((a = 1, b = 2)) == (:a, :b)
@test positional_values_11372(pairs((a = 1, b = 2))...) == (:a => 1, :b => 2)
memory_state_11389 = Memory{Int}(undef, 2)
memory_state_11389[1] = 10
memory_state_11389[2] = 20
@test iterate(memory_state_11389) == (10, 2)
@test iterate(memory_state_11389, 2) == (20, 3)
@test_throws BoundsError positional_values_11372(ScalarStepIter11372()...)
@test_throws BoundsError positional_values_11372(OneFieldStepIter11372()...)
@test positional_values_11372(ExtraTupleStepIter11372()...) == (11,)
@test positional_values_11372(NamedStepIter11372()...) == (12,)
@test positional_values_11372(StructStepIter11372()...) == (13,)
@test positional_values_11372(MutableStructStepIter11372()...) == (14,)
@test positional_values_11372(CatchingInnerIter11372()...) == (17,)
@test_throws MethodError positional_values_11372(SymbolStateOnlyIter11372()...)
@test positional_values_11372(RangeStepResultIter11372()...) == (10,)
step_range_len_field = positional_values_11372(StepRangeLenResultIter11372()...)[1]
@test string(typeof(step_range_len_field)) == "Base.TwicePrecision{Float64}"

seed_dead_box_before_gc_11372()
GC.gc()
mutable_collection = MutableCollectionGc11372((31, 32))
@test positional_values_11372(mutable_collection...) == (31, 32)
yielded_box = positional_values_11372(YieldBoxThenGc11372()...)[1]
@test yielded_box.value == 41
pending_tail = positional_values_11372(GcOnce11372()..., Box11372(42))[2]
@test pending_tail.value == 42
mutable_callable = MutableCallable11372(Box11372(45))
@test mutable_callable(GcOnce11372()...) == 46
closure_box = Box11372(46)
closure_callable = (values...) -> closure_box.value + sum(values)
@test closure_callable(GcOnce11372()...) == 47

array_pairs = [:a => 1, :b => 2]
array_options = keyword_values_11372(; array_pairs...)
@test array_options[:a] == 1
@test array_options[:b] == 2

dict_pairs = Dict(:b => 20, :a => 10)
dict_options = keyword_values_11372(; dict_pairs...)
@test dict_options[:a] == 10
@test dict_options[:b] == 20

two_field_entry_options = keyword_values_11372(; (ThirdStepThrowEntry11372(),)...)
@test two_field_entry_options[:a] == 1

gc_kw_options = keyword_values_11372(; KwYieldBoxThenGc11372()...)
@test gc_kw_options[:a].value == 43

indexed_entry = IndexedOnlyGcEntry11372(:a, Box11372(44))
indexed_options = keyword_values_11372(; (indexed_entry,)...)
@test indexed_options[:a].value == 44

one_field_second_options = keyword_values_11372(; (OneFieldSecondEntry11372(),)...)
@test one_field_second_options[:one] == 9
@test_throws TypeError keyword_values_11372(; (NonSymbolOneFieldSecondEntry11372(),)...)

generic_duplicate_options = keyword_values_11372(; ((:a, 1), (:a, 2))...)
@test generic_duplicate_options[:a] == 2
unique_zip_options = keyword_values_11372(; zip((:a, :b), (1, 2))...)
@test unique_zip_options[:a] == 1
@test unique_zip_options[:b] == 2

@test combined_sum_11372(GcOnce11372()...; a = Box11372(47)) == 48
@test combined_sum_11372(GcOnce11372()...; (a = Box11372(48),)...) == 49

empty_options = keyword_values_11372(; ()...)
@test isempty(empty_options)
@test invalid_keyword_precedes_invalid_positional_11372()
@test valid_keyword_precedes_invalid_positional_11372()
@test evaluation_precedes_keyword_then_positional_validation_11372()
@test malformed_entry_precedes_next_outer_step_11372()
@test second_entry_field_precedes_key_typeassert_11372()
@test catches_iterate_in_calling_frame_11372()
@test catches_iterate_in_ancestor_frame_11372()

end

true
