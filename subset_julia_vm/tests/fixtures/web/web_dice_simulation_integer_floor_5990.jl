function integer_floor(x)
    result = 0
    while result + 1 <= x
        result += 1
    end
    result
end

function simulate_dice(n_rolls)
    rolls = rand(n_rolls)
    sum = 0
    for i in 1:n_rolls
        die = 1 + integer_floor(rolls[i] * 6)
        if die > 6
            die = 6
        end
        sum += die
    end
    sum / n_rolls
end

avg = simulate_dice(128)
avg >= 1.0 && avg <= 6.0
