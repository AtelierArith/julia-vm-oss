module PrecompileTools

export @compile_workload, @setup_workload

# sjulia has no package precompile runtime hook; expose the upstream macro names
# as no-op compatibility wrappers for bundled packages (Issue #7457).
macro compile_workload(ex)
    ex
end

macro setup_workload(ex)
    ex
end

end # module PrecompileTools
