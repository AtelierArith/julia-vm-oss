macro block_tail_if_7604()
    quote
        x = 1
        if x === 1
            true
        else
            false
        end
    end
end

result_7604 = @block_tail_if_7604
result_7604 === true
