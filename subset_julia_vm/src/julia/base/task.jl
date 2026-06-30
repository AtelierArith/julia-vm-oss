# =============================================================================
# task.jl - Task and Concurrency Primitives
# =============================================================================
# Based on Julia's base/task.jl
#
# This implements a cooperative multitasking model suitable for
# single-threaded execution (e.g., iOS without JIT).

# =============================================================================
# Task State Constants
# =============================================================================

const task_state_runnable = Int64(0)
const task_state_done     = Int64(1)
const task_state_failed   = Int64(2)

# =============================================================================
# Task Type
# =============================================================================

"""
    Task

A Task represents a unit of work that can be scheduled and executed.

In SubsetJuliaVM's cooperative multitasking model, tasks are executed
sequentially. The cooperative model has no true concurrency.

# Fields
- `func`: The function to execute
- `state`: Current state (0=runnable, 1=done, 2=failed)
- `result`: The return value or exception
- `_isexception`: Whether result is an exception
- `started`: Whether the task has been started
- `storage`: Task-local storage (Dict or nothing)

# Examples
```julia
t = Task(() -> 1 + 1)
schedule(t)
fetch(t)  # returns 2
```
"""
mutable struct Task
    func::Function
    state::Int64
    result
    _isexception::Bool
    started::Bool
    storage::Any

    function Task(f::Function)
        new(f, 0, nothing, false, false, nothing)
    end
end

# =============================================================================
# Task Status Functions
# =============================================================================

"""
    istaskdone(t::Task) -> Bool

Determine whether a task has exited (completed or failed).
"""
istaskdone(t::Task) = t.state !== 0

"""
    istaskstarted(t::Task) -> Bool

Determine whether a task has started executing.
"""
istaskstarted(t::Task) = t.started

"""
    istaskfailed(t::Task) -> Bool

Determine whether a task has exited because an exception was thrown.
"""
istaskfailed(t::Task) = t.state === 2

# =============================================================================
# Task Scheduling and Execution
# =============================================================================

"""
    _close_bound_channels(t)

Internal: close all channels bound to `t` via `bind(c, t)`.
Called automatically by `schedule` after the task finishes.
"""
function _close_bound_channels(t)
    if t.storage === nothing
        return nothing
    end
    channels = get(t.storage, :__bound_channels__, nothing)
    if channels === nothing
        return nothing
    end
    for c in channels
        if isopen(c)
            if istaskfailed(t)
                close(c, TaskFailedException(t))
            else
                close(c)
            end
        end
    end
    return nothing
end

"""
    schedule(t::Task)

Execute a task immediately in SubsetJuliaVM's cooperative model.
"""
function schedule(t::Task)
    if t.state !== 0
        error("schedule: Task not runnable")
    end
    if t.started
        error("schedule: Task already started")
    end

    t.started = true

    try
        t.result = t.func()
        t.state = 1  # done
    catch e
        t.result = e
        t._isexception = true
        t.state = 2  # failed
    end

    _close_bound_channels(t)

    return t
end

"""
    schedule(t::Task, val; error=false)

Schedule a task with an initial value or as failed.
"""
function schedule(t::Task, val; _error::Bool=false)
    if t.state !== 0
        error("schedule: Task not runnable")
    end

    if _error
        t.result = val
        t._isexception = true
        t.state = 2  # failed
        t.started = true
        _close_bound_channels(t)
    else
        schedule(t)
    end

    return t
end

# =============================================================================
# Waiting and Fetching Results
# =============================================================================

"""
    wait(t::Task)

Block until task `t` is complete. Re-throws any failure as `TaskFailedException`.
"""
function wait(t::Task)
    if !istaskdone(t)
        error("wait: Task not done - schedule the task first")
    end

    if istaskfailed(t)
        throw(TaskFailedException(t))
    end

    return nothing
end

"""
    fetch(t::Task)

Wait for a Task to finish, then return its result value.
If the task fails, a `TaskFailedException` is thrown.
"""
function fetch(t::Task)
    wait(t)
    return t.result
end

"""
    fetch(x)

For non-Task values, simply return the value.
"""
fetch(x) = x

# =============================================================================
# Task Result Access
# =============================================================================

"""
    task_result(t::Task)

Get the result of a completed task. Throws `TaskFailedException` if the task failed.
"""
function task_result(t::Task)
    if t._isexception
        throw(TaskFailedException(t))
    end
    return t.result
end

# =============================================================================
# Current Task
# =============================================================================

# Helper for the main task's no-op function (arrow functions not supported at module load)
_main_task_noop() = nothing

"""
    current_task() -> Task

Return the currently running Task.
In SubsetJuliaVM's cooperative model this returns a new main task singleton each call.
"""
function current_task()
    main = Task(_main_task_noop)
    main.started = true
    return main
end

# =============================================================================
# Task-Local Storage
# =============================================================================

function get_task_tls(t::Task)
    if t.storage === nothing
        t.storage = Dict()
    end
    return t.storage
end

"""
    task_local_storage() -> Dict

Return the task-local storage dictionary for the current task.
"""
task_local_storage() = get_task_tls(current_task())

"""
    task_local_storage(key)

Look up the value of `key` in the current task's task-local storage.
"""
task_local_storage(key) = task_local_storage()[key]

"""
    task_local_storage(key, value)

Assign `value` to `key` in the current task's task-local storage.
"""
function task_local_storage(key, val)
    tls = task_local_storage()
    tls[key] = val
    return val
end

"""
    task_local_storage(body, key, value)

Call `body` with a modified task-local storage where `key` is bound to `value`.
The previous binding is restored afterwards.
"""
function task_local_storage(body::Function, key, val)
    tls = task_local_storage()
    hadkey = haskey(tls, key)
    old = get(tls, key, nothing)
    tls[key] = val
    try
        return body()
    finally
        if hadkey
            tls[key] = old
        else
            delete!(tls, key)
        end
    end
end

# =============================================================================
# Yield
# =============================================================================

"""
    yield()

No-op in SubsetJuliaVM's cooperative model.
"""
function yield()
    return nothing
end

"""
    yield(t::Task)

Schedule `t` and yield to it. In SubsetJuliaVM runs the task immediately.
"""
function yield(t::Task)
    schedule(t)
    return nothing
end

"""
    yieldto(t::Task, val=nothing)

Yield to task `t`. In SubsetJuliaVM this is a no-op.
"""
function yieldto(t::Task, val=nothing)
    return val
end

# =============================================================================
# Wait Multiple Tasks
# =============================================================================

"""
    waitany(tasks; throw=true) -> (done_tasks, remaining_tasks)

Return tasks partitioned into done and remaining.
Since SubsetJuliaVM tasks execute immediately on scheduling, all scheduled
tasks are already done when this is called.

If `throw` is `true`, throws `CompositeException` when any done task failed.
"""
function waitany(tasks; _throw::Bool=true)
    done_tasks = Task[]
    remaining_tasks = Task[]
    exceptions = Any[]

    for t in tasks
        if istaskdone(t)
            push!(done_tasks, t)
            if istaskfailed(t)
                push!(exceptions, TaskFailedException(t))
            end
        else
            push!(remaining_tasks, t)
        end
    end

    if _throw && !isempty(exceptions)
        throw(CompositeException(exceptions))
    end

    return (done_tasks, remaining_tasks)
end

"""
    waitall(tasks; failfast=true, throw=true) -> (done_tasks, remaining_tasks)

Wait for all given tasks to complete.
Since SubsetJuliaVM tasks execute immediately, this inspects tasks after scheduling.

If `throw` is `true`, throws `CompositeException` on any failure.
"""
function waitall(tasks; failfast::Bool=true, _throw::Bool=true)
    done_tasks = Task[]
    remaining_tasks = Task[]
    exceptions = Any[]

    for t in tasks
        if istaskdone(t)
            push!(done_tasks, t)
            if istaskfailed(t)
                push!(exceptions, TaskFailedException(t))
                if failfast
                    break
                end
            end
        else
            push!(remaining_tasks, t)
        end
    end

    if _throw && !isempty(exceptions)
        throw(CompositeException(exceptions))
    end

    return (done_tasks, remaining_tasks)
end

# =============================================================================
# Error Monitor
# =============================================================================

"""
    errormonitor(t::Task) -> Task

If task `t` has failed, print an error to stderr.
"""
function errormonitor(t::Task)
    if istaskfailed(t)
        println(stderr, "Unhandled Task ERROR: ", string(t.result))
    end
    return t
end

# =============================================================================
# Condition (notify extension — Condition struct is defined in lock.jl)
# =============================================================================
# The Condition struct with `waiting::Int64` is defined in lock.jl.
# Here we provide a no-op `wait(c::Condition)` override and an extended
# `notify` that accepts optional value / keyword arguments.
# Note: Condition in lock.jl does not have a task waitq, so notify is a no-op.

"""
    wait(c::Condition; first::Bool=false)

No-op in SubsetJuliaVM's cooperative model (true blocking requires coroutines).
"""
function wait(c::Condition; first::Bool=false)
    return nothing
end

"""
    notify(c::Condition, val=nothing; all::Bool=true, error::Bool=false) -> Int

No-op in SubsetJuliaVM (no tasks are truly waiting on a Condition).
Returns 0.
"""
function notify(c::Condition, val=nothing; all::Bool=true, _error::Bool=false)
    return 0
end

# =============================================================================
# timedwait (Issue #3501)
# =============================================================================
# Based on Julia's base/asyncevent.jl. Polls `testcb()` until it returns true
# or `timeout` seconds elapse, sleeping `pollint` seconds between polls.
# Returns `:ok` if the predicate became true, `:timed_out` otherwise.
#
# In SubsetJuliaVM's single-threaded cooperative model, `sleep` blocks the
# whole VM, so the predicate is only re-evaluated after each `sleep(pollint)`
# returns. This matches Julia's observable behavior for purely time-driven
# predicates (the common use case).

"""
    timedwait(testcb, timeout::Real; pollint::Real=0.1)

Wait until `testcb()` returns `true` or `timeout` seconds have elapsed,
whichever is earlier. `testcb` is polled every `pollint` seconds. The minimum
value for `pollint` is 0.001 seconds, that is, 1 millisecond.

Returns `:ok` if the test condition was met before timing out, or
`:timed_out` otherwise.
"""
function timedwait(testcb, timeout::Real; pollint::Real=0.1)
    pollint >= 1e-3 || throw(ArgumentError("pollint must be ≥ 1 millisecond"))
    start = time_ns()
    ns_timeout = 1.0e9 * Float64(timeout)
    testcb() && return :ok
    while Float64(time_ns() - start) < ns_timeout
        sleep(pollint)
        testcb() && return :ok
    end
    return :timed_out
end

# =============================================================================
# @task, @async, @sync (lowering-implemented)
# =============================================================================
# Handled by the lowering layer in src/lowering/expr/macros/mod.rs
