# Receiver-sensitive getindex inference matrix (Issue #10887).

using Test

result_kind(::String) = :string
result_kind(::AbstractArray) = :array
result_kind(::Tuple) = :tuple
result_kind(::AbstractRange) = :range
result_kind(::Any) = :other
result_kind(::String, ::Int) = :string
result_kind(::AbstractArray, ::Int) = :array
result_kind(::Any, ::Int) = :other

struct CustomIndexReceiver10887 end

Base.getindex(::CustomIndexReceiver10887, ::Int) = "scalar-key"
Base.getindex(::CustomIndexReceiver10887, ::UnitRange{Int}) = "range-key"
Base.getindex(::CustomIndexReceiver10887, ::Vector{Int}) = "int-vector-key"
Base.getindex(::CustomIndexReceiver10887, ::Vector{Bool}) = "bool-vector-key"

typed_custom_int_vector_kind_10887(receiver::CustomIndexReceiver10887, index::Vector{Int}) =
    result_kind(receiver[index], 10887)
unknown_custom_int_vector_kind_10887(receiver::Any, index::Vector{Int}) =
    result_kind(receiver[index], 10887)
typed_custom_bool_vector_kind_10887(receiver::CustomIndexReceiver10887, index::Vector{Bool}) =
    result_kind(receiver[index], 10887)
typed_weakdict_bool_vector_kind_10887(receiver::WeakKeyDict{Any,String}, index::Vector{Bool}) =
    result_kind(receiver[index], 10887)

@testset "receiver-sensitive getindex inference matrix (Issue #10887)" begin
    values = [10, 20, 30]
    @test values[2] == 20
    @test typeof(values[2]) == Int64
    @test result_kind(values[2]) == :other
    @test values[2:3] == [20, 30]
    @test typeof(values[2:3]) == Vector{Int64}
    @test result_kind(values[2:3]) == :array
    @test values[[1, 3]] == [10, 30]
    @test typeof(values[[1, 3]]) == Vector{Int64}
    @test result_kind(values[[1, 3]]) == :array
    @test values[Bool[true, false, true]] == [10, 30]
    @test typeof(values[Bool[true, false, true]]) == Vector{Int64}
    @test result_kind(values[Bool[true, false, true]]) == :array

    range_values = 1:5
    @test range_values[2] == 2
    @test typeof(range_values[2]) == Int64
    @test result_kind(range_values[2]) == :other
    @test range_values[2:3] == 2:3
    @test typeof(range_values[2:3]) == UnitRange{Int64}
    @test result_kind(range_values[2:3]) == :range
    @test range_values[[1, 3]] == [1, 3]
    @test typeof(range_values[[1, 3]]) == Vector{Int64}
    @test result_kind(range_values[[1, 3]]) == :array
    @test range_values[Bool[true, false, true, false, true]] == [1, 3, 5]
    @test typeof(range_values[Bool[true, false, true, false, true]]) == Vector{Int64}
    @test result_kind(range_values[Bool[true, false, true, false, true]]) == :array

    tuple_values = ("a", "b", "c")
    @test tuple_values[2] == "b"
    @test typeof(tuple_values[2]) == String
    @test result_kind(tuple_values[2]) == :string
    @test tuple_values[1:2] == ("a", "b")
    @test typeof(tuple_values[1:2]) <: Tuple
    @test result_kind(tuple_values[1:2]) == :tuple
    tuple_int_vector_value = tuple_values[[1, 3]]
    @test tuple_int_vector_value == ("a", "c")
    @test tuple_values[[1, 3]] == ("a", "c")
    @test typeof(tuple_values[[1, 3]]) <: Tuple
    @test result_kind(tuple_values[[1, 3]]) == :tuple
    tuple_bool_vector_value = tuple_values[Bool[true, false, true]]
    @test tuple_bool_vector_value == ("a", "c")
    @test tuple_values[Bool[true, false, true]] == ("a", "c")
    @test typeof(tuple_values[Bool[true, false, true]]) <: Tuple
    @test result_kind(tuple_values[Bool[true, false, true]]) == :tuple

    weakdict = WeakKeyDict{Any,String}()
    array_key = [1, 3]
    weakdict[array_key] = "ARRAY-KEY"
    @test weakdict[array_key] == "ARRAY-KEY"
    @test typeof(weakdict[array_key]) == String
    @test result_kind(weakdict[array_key]) == :string
    bool_array_key = Bool[true, false]
    weakdict[bool_array_key] = "BOOL-ARRAY-KEY"
    bool_array_key_value = weakdict[bool_array_key]
    @test bool_array_key_value == "BOOL-ARRAY-KEY"
    @test weakdict[bool_array_key] == "BOOL-ARRAY-KEY"
    @test typeof(weakdict[bool_array_key]) == String
    @test result_kind(weakdict[bool_array_key]) == :string
    @test typed_weakdict_bool_vector_kind_10887(weakdict, bool_array_key) == :string

    custom = CustomIndexReceiver10887()
    @test custom[2] == "scalar-key"
    @test typeof(custom[2]) == String
    @test result_kind(custom[2]) == :string
    @test custom[1:2] == "range-key"
    @test typeof(custom[1:2]) == String
    @test result_kind(custom[1:2]) == :string
    @test custom[[1, 3]] == "int-vector-key"
    @test typeof(custom[[1, 3]]) == String
    @test result_kind(custom[[1, 3]]) == :string
    @test typed_custom_int_vector_kind_10887(custom, [1, 3]) == :string
    @test unknown_custom_int_vector_kind_10887(custom, [1, 3]) == :string
    @test custom[Bool[true, false]] == "bool-vector-key"
    @test typeof(custom[Bool[true, false]]) == String
    @test result_kind(custom[Bool[true, false]]) == :string
    @test typed_custom_bool_vector_kind_10887(custom, Bool[true, false]) == :string
end

true
