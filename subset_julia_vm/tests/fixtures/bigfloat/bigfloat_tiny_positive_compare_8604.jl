function bigfloat_tiny_positive_compare_8604()
    tiny = big"1e-78"
    zero_big = big"0.0"

    return tiny > zero_big &&
        tiny >= zero_big &&
        !(tiny < zero_big) &&
        !(tiny <= zero_big) &&
        tiny != zero_big &&
        zero_big < tiny &&
        zero_big <= tiny &&
        !(zero_big > tiny) &&
        !(zero_big >= tiny) &&
        -tiny < zero_big &&
        -tiny <= zero_big &&
        !(-tiny > zero_big) &&
        !(-tiny >= zero_big)
end

bigfloat_tiny_positive_compare_8604()
