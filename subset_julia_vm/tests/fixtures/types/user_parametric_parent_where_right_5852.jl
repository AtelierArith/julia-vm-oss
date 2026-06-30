using Test

abstract type WrapperWhereRight5852{S} end
struct MyVecWhereRight5852{T} <: WrapperWhereRight5852{T}
    value::T
end

@test (MyVecWhereRight5852{T} where T) <: (WrapperWhereRight5852{S} where S)
@test MyVecWhereRight5852{Int64} <: (WrapperWhereRight5852{S} where S)
@test MyVecWhereRight5852{Int64} <: WrapperWhereRight5852{Int64}
@test !(MyVecWhereRight5852{Int64} <: WrapperWhereRight5852{Real})

@test MyVecWhereRight5852{Int64} <: (WrapperWhereRight5852{S} where S<:Real)
@test !(MyVecWhereRight5852{String} <: (WrapperWhereRight5852{S} where S<:Real))

true
