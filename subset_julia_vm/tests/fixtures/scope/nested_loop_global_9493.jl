using Test

val = 1
for i in 1:2
    for j in 1:3
        global val += 1
    end
end

@test val == 7

true
