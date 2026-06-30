# Type Inference Examples
#
# AoT の推論が “静的” に寄れるような書き方の例。
# ここでは動的サイズの push! 等は避け、型が決まりやすい形に寄せる。

function compute_area_circle(radius::Float64)::Float64
    return 3.14159265358979 * radius * radius
end

function compute_area_rectangle(width::Float64, height::Float64)::Float64
    return width * height
end

function is_positive(x::Float64)::Bool
    return x > 0.0
end

function quadratic_formula(a::Float64, b::Float64, c::Float64)
    discriminant = b * b - 4.0 * a * c
    if discriminant < 0.0
        # NaN
        return (0.0 / 0.0, 0.0 / 0.0)
    end
    sqrt_disc = sqrt(discriminant)
    denom = 2.0 * a
    x1 = (-b + sqrt_disc) / denom
    x2 = (-b - sqrt_disc) / denom
    return (x1, x2)
end

function newton_sqrt(x::Float64, iterations::Int64)::Float64
    if x <= 0.0
        return 0.0
    end
    guess = x / 2.0
    for _ in 1:iterations
        guess = (guess + x / guess) / 2.0
    end
    return guess
end

function array_map_square(arr::Vector{Float64})::Vector{Float64}
    n = length(arr)
    result = zeros(n)
    for i in 1:n
        result[i] = arr[i] * arr[i]
    end
    return result
end

function count_positive(arr::Vector{Float64})::Int64
    n = length(arr)
    count = 0
    for i in 1:n
        if arr[i] > 0.0
            count += 1
        end
    end
    return count
end

function running_average(arr::Vector{Float64})::Vector{Float64}
    n = length(arr)
    result = zeros(n)
    running_sum = 0.0
    for i in 1:n
        running_sum += arr[i]
        result[i] = running_sum / Float64(i)
    end
    return result
end

function exponential_moving_average(arr::Vector{Float64}, alpha::Float64)::Vector{Float64}
    n = length(arr)
    if n == 0
        return Float64[]
    end

    result = zeros(n)
    ema = arr[1]
    result[1] = ema

    one_minus_alpha = 1.0 - alpha
    for i in 2:n
        ema = alpha * arr[i] + one_minus_alpha * ema
        result[i] = ema
    end
    return result
end

function main()
    compute_area_circle(1.0)
    compute_area_rectangle(3.0, 4.0)
    is_positive(-1.0)
    quadratic_formula(1.0, -5.0, 6.0)
    newton_sqrt(4.0, 10)
    array_map_square([1.0, 2.0, 3.0])
    count_positive([-1.0, 2.0, -3.0, 4.0])
    running_average([1.0, 2.0, 3.0])
    exponential_moving_average([1.0, 2.0, 3.0], 0.5)
    true
end

