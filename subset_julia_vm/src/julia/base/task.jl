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

# Helper for the main task's no-op function (arrow functions not supported at module load)
_main_task_noop() = nothing

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
- `queued`: Whether the task is waiting in the runnable queue
- `storage`: Task-local storage (Dict or nothing)
- `vm_id`: VM scheduler slot for this task
- `waiters`: Tasks parked until this task exits
- `error_monitored`: Whether a future failure should be reported to stderr

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
    queued::Bool
    storage::Any
    vm_id::Int64
    waiters::Vector{Any}
    error_monitored::Bool

    function Task(f::Function)
        new(f, 0, nothing, false, false, false, nothing, -1, Any[], false)
    end
end

const __sjulia_current_task_cell__ = Any[nothing]

function __sjulia_main_task()
    current = __sjulia_current_task_cell__[1]
    if current === nothing
        main = Task(_main_task_noop)
        main.started = true
        main.vm_id = _task_register_main(main)
        __sjulia_current_task_cell__[1] = main
        return main
    end
    return current
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

function __sjulia_finish_task(t::Task)
    if t._isexception && t.error_monitored
        println(stderr, "Unhandled Task ERROR: ", string(t.result))
    end
    _close_bound_channels(t)
    waiters = t.waiters
    t.waiters = Any[]
    for waiter in waiters
        _task_wake(waiter.vm_id)
    end
    return nothing
end

"""
    schedule(t::Task)

Mark a task runnable. The task body runs when the scheduler reaches a yield/wait
point, matching Julia's observable schedule-before-run behavior on a single
thread (Issue #8989).
"""
function schedule(t::Task)
    if t.state !== 0
        error("schedule: Task not runnable")
    end
    if t.started
        error("schedule: Task already started")
    end
    if t.queued
        error("schedule: Task not runnable")
    end

    __sjulia_main_task()  # register task 0 before the first child task
    t.vm_id = _task_schedule(t, __sjulia_task_entry)
    t.queued = true
    return t
end

function __sjulia_task_entry(t::Task)
    if t.state !== 0
        return t
    end
    t.queued = false
    t.started = true

    try
        t.result = t.func()
        t.state = 1  # done
    catch e
        t.result = e
        t._isexception = true
        t.state = 2  # failed
    end

    __sjulia_finish_task(t)

    return t
end

# Compatibility entry used by older Base code: drive the VM scheduler rather
# than calling the task body recursively/run-to-completion.
function __sjulia_run_task(t::Task)
    wait(t)
    return t
end

function __sjulia_run_one_task()
    _task_yield()
    return true
end

function __sjulia_run_until_done(t::Task)
    while !istaskdone(t)
        wait(t)
    end
    return nothing
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
        t.queued = false
        __sjulia_finish_task(t)
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
    if !istaskdone(t) && t.vm_id < 0
        error("wait: Task not done - schedule the task first")
    end

    while !istaskdone(t)
        waiter = current_task()
        push!(t.waiters, waiter)
        _task_park()
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

"""
    current_task() -> Task

Return the currently running Task.
"""
function current_task()
    __sjulia_main_task()
    return _task_current()
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

Suspend at this exact yield point, run the next runnable VM task, and resume
this task later (Issue #10349).
"""
function yield()
    _task_yield()
    return nothing
end

"""
    yield(t::Task)

Schedule `t` and cooperatively wait for it.
"""
function yield(t::Task)
    if !t.started && !t.queued && t.state === 0
        schedule(t)
    end
    wait(t)
    return nothing
end

"""
    yieldto(t::Task, val=nothing)

Yield to task `t` through the cooperative scheduler.
"""
function yieldto(t::Task, val=nothing)
    if !t.started && !t.queued && t.state === 0
        schedule(t)
    end
    yield()
    return val
end

# =============================================================================
# Wait Multiple Tasks
# =============================================================================

"""
    waitany(tasks; throw=true) -> (done_tasks, remaining_tasks)

Return tasks partitioned into done and remaining.
If none are done, cooperatively yield until at least one scheduled task exits.

If `throw` is `true`, throws `CompositeException` when any done task failed.
"""
function waitany(tasks; _throw::Bool=true)
    while true
        any_done = false
        for t in tasks
            if istaskdone(t)
                any_done = true
            elseif t.vm_id < 0
                error("waitany: encountered an unscheduled task")
            end
        end
        if any_done || isempty(tasks)
            break
        end
        yield()
    end

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
Each incomplete task parks the caller until its completion wakeup.

If `throw` is `true`, throws `CompositeException` on any failure.
"""
function waitall(tasks; failfast::Bool=true, _throw::Bool=true)
    for t in tasks
        if !istaskdone(t)
            wait(t)
        end
        if failfast && istaskfailed(t)
            break
        end
    end

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
    t.error_monitored = true
    if istaskfailed(t)
        println(stderr, "Unhandled Task ERROR: ", string(t.result))
    end
    return t
end

# =============================================================================
# Condition (notify extension — Condition struct is defined in lock.jl)
# =============================================================================
# The Condition struct and its blocking `wait` method are defined in lock.jl.
# This extended `notify` wakes one or all parked VM continuations.

"""
    wait(c::Condition; first::Bool=false)

Park the current task until `notify` wakes it.
"""
function wait(c::Condition; first::Bool=false)
    return _wait_condition(c)
end

"""
    notify(c::Condition, val=nothing; all::Bool=true, error::Bool=false) -> Int

Wake one or all tasks waiting on `c` and return the number woken.
"""
function notify(c::Condition, val=nothing; all::Bool=true, _error::Bool=false)
    c.value = val
    waiters = c.waitq
    if isempty(waiters)
        return 0
    end
    count = all ? length(waiters) : 1
    for i in 1:count
        _task_wake(waiters[i].vm_id)
    end
    c.waitq = waiters[count + 1:end]
    return count
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
