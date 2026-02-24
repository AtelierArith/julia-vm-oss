# Test ! operator in macro-generated code (Issue 290)
macro myassert(cond, msg)
    quote
        if !($(esc(cond)))
            error($(esc(msg)))
        end
    end
end

# Should not error
@myassert true "should not fail"
@myassert 1 + 1 == 2 "math works"

true
