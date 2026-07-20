using MacroTools: @capture

function assert_capture_function_expr_9055(ex)
    @assert @capture(ex, (fcall_ = body_))
    Expr(:function, fcall, body)
end

result = assert_capture_function_expr_9055(:(f(x) = x))

result.head === :function &&
    result.args[1] == :(f(x)) &&
    result.args[2] == :x
