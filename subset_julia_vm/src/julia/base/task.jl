# =============================================================================
# task.jl - Task and Concurrency Primitives
# =============================================================================
# Based on Julia's base/task.jl
#
# This implements a simplified cooperative multitasking model suitable for
# single-threaded execution (e.g., iOS without JIT).
#
# Note: SubsetJuliaVM runs on a single thread, so this implements a
# cooperative multitasking model where tasks yield control explicitly.

# =============================================================================
# Task State Constants
# =============================================================================

"""
    TaskState

Enumeration of possible task states.
Note: These const values are defined for documentation but cannot be accessed
from function bodies due to SubsetJuliaVM limitations (Issue #1443).
Functions use literal values (0, 1, 2) instead.
- 0 = runnable
- 1 = done
- 2 = failed
"""
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
sequentially - there is no true parallelism. Tasks yield control explicitly
and are resumed when scheduled.

# Fields
- `func`: The function to execute
- `state`: Current state (runnable, done, or failed)
- `result`: The return value or exception
- `_isexception`: Whether result is an exception
- `started`: Whether the task has been started

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

    function Task(f::Function)
        # Use literal 0 (runnable) instead of task_state_runnable due to VM limitation
        new(f, 0, nothing, false, false)
    end
end

# =============================================================================
# Task Status Functions
# =============================================================================

"""
    istaskdone(t::Task) -> Bool

Determine whether a task has exited (completed or failed).

# Examples
```julia
t = Task(() -> 1 + 1)
istaskdone(t)  # false
schedule(t)
istaskdone(t)  # true
```
"""
# Use literal 0 (runnable) instead of task_state_runnable due to VM limitation
istaskdone(t::Task) = t.state !== 0

"""
    istaskstarted(t::Task) -> Bool

Determine whether a task has started executing.

# Examples
```julia
t = Task(() -> 1 + 1)
istaskstarted(t)  # false
schedule(t)
istaskstarted(t)  # true
```
"""
istaskstarted(t::Task) = t.started

"""
    istaskfailed(t::Task) -> Bool

Determine whether a task has exited because an exception was thrown.

# Examples
```julia
t = Task(() -> error("oops"))
schedule(t)
istaskfailed(t)  # true
```
"""
# Use literal 2 (failed) instead of task_state_failed due to VM limitation
istaskfailed(t::Task) = t.state === 2

# =============================================================================
# Task Scheduling and Execution
# =============================================================================

"""
    schedule(t::Task)

Add a task to the scheduler's queue, or run it immediately in
SubsetJuliaVM's cooperative model.

In SubsetJuliaVM, this immediately executes the task since there is no
true task scheduler.

# Examples
```julia
t = Task(() -> println("Hello"))
schedule(t)  # Prints "Hello"
```
"""
function schedule(t::Task)
    # Use literal values instead of task_state_* due to VM limitation
    # 0 = runnable, 1 = done, 2 = failed
    if t.state !== 0
        error("schedule: Task not runnable")
    end
    if t.started
        error("schedule: Task already started")
    end

    # Mark as started
    t.started = true

    # Execute immediately (cooperative model)
    try
        t.result = t.func()
        t.state = 1  # done
    catch e
        t.result = e
        t._isexception = true
        t.state = 2  # failed
    end

    return t
end

"""
    schedule(t::Task, val; error=false)

Add a task to the scheduler's queue with an initial value or exception.

# Arguments
- `t`: The task to schedule
- `val`: The value to provide to the task
- `error`: If true, treat `val` as an exception

# Examples
```julia
t = Task(() -> 42)
schedule(t, nothing)
```
"""
function schedule(t::Task, val; error::Bool=false)
    # Use literal values instead of task_state_* due to VM limitation
    # 0 = runnable, 2 = failed
    if t.state !== 0
        Base.error("schedule: Task not runnable")
    end

    if error
        t.result = val
        t._isexception = true
        t.state = 2  # failed
        t.started = true
    else
        # For simplicity, ignore the provided value and just run the task
        schedule(t)
    end

    return t
end

# =============================================================================
# Waiting and Fetching Results
# =============================================================================

"""
    wait(t::Task)

Block the current task until the specified task `t` is complete.

In SubsetJuliaVM's cooperative model, since tasks execute immediately when
scheduled, this function simply checks if the task is done.

# Examples
```julia
t = Task(() -> 1 + 1)
schedule(t)
wait(t)  # Returns immediately since task is already done
```
"""
function wait(t::Task)
    if !istaskdone(t)
        error("wait: Task not done - in cooperative model, tasks must be scheduled first")
    end

    if istaskfailed(t)
        throw(TaskFailedException(string(t.result)))
    end

    return nothing
end

"""
    fetch(t::Task)

Wait for a Task to finish, then return its result value.
If the task fails with an exception, a `TaskFailedException` is thrown.

# Examples
```julia
t = Task(() -> 1 + 1)
schedule(t)
fetch(t)  # returns 2
```
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
# Current Task (Stub)
# =============================================================================

# In SubsetJuliaVM's cooperative model, there is conceptually always a "main" task
# We use a global to track the current task, but in practice this is rarely needed

"""
    current_task()

Get a reference to the currently running Task.

Note: In SubsetJuliaVM's cooperative model, this returns a placeholder
since there is no true task switching.
"""
function current_task()
    # Return a dummy task representing the main execution context
    # This is a simplified implementation for compatibility
    error("current_task() is not fully supported in SubsetJuliaVM's cooperative model")
end

# =============================================================================
# Yield (Stub)
# =============================================================================

"""
    yield()

Switch to the scheduler to allow another scheduled task to run.

In SubsetJuliaVM's cooperative model, this is a no-op since tasks execute
immediately when scheduled.
"""
function yield()
    # No-op in cooperative model
    return nothing
end

"""
    yield(t::Task)

A fast, unfair-Loss version of `schedule(t); yield()` which
immediately yields to `t` before calling the scheduler.

In SubsetJuliaVM's cooperative model, this schedules and runs the task.
"""
function yield(t::Task)
    schedule(t)
    return nothing
end

# =============================================================================
# Task Result Access
# =============================================================================

"""
    task_result(t::Task)

Get the result of a completed task. Throws if the task failed.
"""
function task_result(t::Task)
    if t._isexception
        throw(TaskFailedException(string(t.result)))
    end
    return t.result
end

# =============================================================================
# @task Macro
# =============================================================================
# Note: This macro is defined but its implementation depends on the lowering
# layer supporting macro expansion. For now, we provide a function-based API.

# The @task macro would wrap an expression in a Task:
# @task expr -> Task(() -> expr)
