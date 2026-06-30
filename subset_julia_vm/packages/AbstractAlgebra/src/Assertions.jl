################################################################################
#
#     Assertions.jl : custom assertions
#
################################################################################

@doc raw"""
    @req(assert, msg)
    @req assert msg

Check whether the assertion `assert` is true. If not, throw an `ArgumentError`
with error message `msg`.
"""
macro req(cond, msg)
  quote
    if !($(esc(cond)))
      throw(ArgumentError($(esc(msg))))
    end
  end
end
