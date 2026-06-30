import Base: collect

generator_collect_user_double_4265(x) = x * 2
generator_collect_user_runtime_4265(x::Any) = collect(x)

collect(g::Base.Generator) = :generator_dispatch_4265

direct = collect(Base.Generator(generator_collect_user_double_4265, [1, 2, 3]))
runtime = generator_collect_user_runtime_4265(
    Base.Generator(generator_collect_user_double_4265, [1, 2, 3]),
)

if direct !== :generator_dispatch_4265
    error("direct collect(Base.Generator(...)) did not dispatch to user method")
end

if runtime !== :generator_dispatch_4265
    error("runtime collect(x::Any) did not dispatch to user method")
end

true
