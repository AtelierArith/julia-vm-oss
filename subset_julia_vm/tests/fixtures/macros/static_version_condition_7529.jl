old_enough = @static if VERSION >= v"0.1.0"
    true
else
    false
end

future = @static if VERSION >= v"1.12-"
    true
else
    false
end

old_enough && !future
