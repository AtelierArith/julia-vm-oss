using Test

struct ObjectidDispatchBox3911
    n::Int64
end

Base.objectid(::ObjectidDispatchBox3911) = 3911

box = ObjectidDispatchBox3911(7)

@test Base.objectid(box) == 3911
@test typeof(Base.objectid(:objectid_fallback_3911)) === UInt64

true
