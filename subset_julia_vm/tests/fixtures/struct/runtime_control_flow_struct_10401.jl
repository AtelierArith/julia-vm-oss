using Test

if false
    struct UntakenStruct10401
        x::Int
    end
else
    struct TakenStruct10401
        y::Int
    end
end

for i in 1:1
    struct ForStruct10401
        x::Int
    end
end

i10401 = 0
while i10401 < 1
    struct WhileStruct10401
        x::Int
    end
    global i10401 += 1
end

try
    struct TryStruct10401
        x::Int
    end
catch
end

@test !isdefined(Main, :UntakenStruct10401)
@test isdefined(Main, :TakenStruct10401)
@test TakenStruct10401(1).y == 1
@test ForStruct10401(2).x == 2
@test WhileStruct10401(3).x == 3
@test TryStruct10401(4).x == 4

true
