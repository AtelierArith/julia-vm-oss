function drive_twice(f)
    first = f(1)
    second = f(2)
    (first, second)
end

function mutates_outer_from_do_block_if()
    result = false
    values = drive_twice() do y
        if y == 2
            result = true
        end
    end
    result && values[1] === nothing && values[2] === true
end

mutates_outer_from_do_block_if()
