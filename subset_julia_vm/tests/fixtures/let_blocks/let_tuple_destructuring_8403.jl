function let_tuple_destructuring_contract_8403()
    a = 100
    b = 200

    simple = let (x, y) = (1, 2)
        x + y
    end

    ordered = let (x, y) = (3, 4), z = x * y
        z + x + y
    end

    nested = let ((x, y), z) = ((5, 6), 7)
        x * 100 + y * 10 + z
    end

    simple == 3 && ordered == 19 && nested == 567 && a == 100 && b == 200
end

let_tuple_destructuring_contract_8403()
