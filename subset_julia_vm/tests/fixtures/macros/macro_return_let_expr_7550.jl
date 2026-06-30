macro single_let()
    return :(let x = 1
        x
    end)
end

macro multi_let()
    return :(let x = 1, y = 2
        x + y
    end)
end

(@single_let) == 1 && (@multi_let) == 3
