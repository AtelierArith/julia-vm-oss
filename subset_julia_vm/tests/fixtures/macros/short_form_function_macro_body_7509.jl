macro short_form_false()
    false
end

f(x) = x ? true : @short_form_false()

f(false) === false
