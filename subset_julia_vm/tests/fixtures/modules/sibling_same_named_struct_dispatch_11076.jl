# Method dispatch must preserve the owner of module-qualified same-named
# structs while still allowing a module-local bare annotation to match its
# own qualified runtime type (Issue #11076).

using Test

module DispatchOwnerA11076
export Box, Plain, local_owner

struct Box{T}
    x::T
end


struct Plain
    x::Int
end


local_owner(::Box) = :local_a
end

module DispatchOwnerB11076
export Box, Plain, local_owner

struct Box{T}
    x::T
end


struct Plain
    x::Int
end


local_owner(::Box) = :local_b
end

# Deliberately declare the B methods first: source order must not select a
# sibling type after both qualified names have the same bare tail.
family_owner(::DispatchOwnerB11076.Box) = :family_b
family_owner(::DispatchOwnerA11076.Box) = :family_a

applied_owner(::DispatchOwnerB11076.Box{Int}) = :applied_b_int
applied_owner(::DispatchOwnerA11076.Box{Int}) = :applied_a_int
applied_owner(::DispatchOwnerB11076.Box) = :applied_b_other
applied_owner(::DispatchOwnerA11076.Box) = :applied_a_other

plain_owner(::DispatchOwnerB11076.Plain) = :plain_b
plain_owner(::DispatchOwnerA11076.Plain) = :plain_a

# Force CallDynamic through an untyped wrapper, and reverse declaration order
# in a second table so neither source order nor a warmed dispatch cache can
# select a sibling owner.
dynamic_family_owner(x) = family_owner(x)
reverse_owner(::DispatchOwnerA11076.Box) = :reverse_a
reverse_owner(::DispatchOwnerB11076.Box) = :reverse_b
dynamic_reverse_owner(x) = reverse_owner(x)

module ExactInner11076
struct Box{T}
    x::T
end
struct Plain
    x::Int
end
end

module ExactOuter11076
module ExactInner11076
struct Box{T}
    x::T
end
struct Plain
    x::Int
end
end
end

exact_owner(::ExactInner11076.Box) = :top_box
exact_owner(::ExactOuter11076.ExactInner11076.Box) = :nested_box
exact_plain_owner(::ExactInner11076.Plain) = :top_plain
exact_plain_owner(::ExactOuter11076.ExactInner11076.Plain) = :nested_plain

@testset "sibling module struct owners participate in dispatch (Issue #11076)" begin
    a_int = DispatchOwnerA11076.Box(1)
    b_int = DispatchOwnerB11076.Box(2)
    a_float = DispatchOwnerA11076.Box(1.5)
    b_float = DispatchOwnerB11076.Box(2.5)

    @test family_owner(a_int) == :family_a
    @test family_owner(b_int) == :family_b
    @test applied_owner(a_int) == :applied_a_int
    @test applied_owner(b_int) == :applied_b_int
    @test applied_owner(a_float) == :applied_a_other
    @test applied_owner(b_float) == :applied_b_other
    @test plain_owner(DispatchOwnerA11076.Plain(3)) == :plain_a
    @test plain_owner(DispatchOwnerB11076.Plain(4)) == :plain_b

    # A bare annotation inside the defining module is the same declaration as
    # the qualified runtime type and must remain applicable.
    @test DispatchOwnerA11076.local_owner(a_int) == :local_a
    @test DispatchOwnerB11076.local_owner(b_int) == :local_b


    @test dynamic_family_owner(a_int) == :family_a
    @test dynamic_family_owner(b_int) == :family_b
    @test dynamic_family_owner(a_float) == :family_a
    @test dynamic_family_owner(b_float) == :family_b
    @test dynamic_family_owner(a_int) == :family_a


    @test dynamic_reverse_owner(a_int) == :reverse_a
    @test dynamic_reverse_owner(b_int) == :reverse_b
    @test dynamic_reverse_owner(a_float) == :reverse_a
    @test dynamic_reverse_owner(b_float) == :reverse_b
    @test dynamic_reverse_owner(a_int) == :reverse_a


    @test exact_owner(ExactInner11076.Box(1)) == :top_box
    @test exact_owner(ExactOuter11076.ExactInner11076.Box(1)) == :nested_box
    @test exact_plain_owner(ExactInner11076.Plain(1)) == :top_plain
    @test exact_plain_owner(ExactOuter11076.ExactInner11076.Plain(1)) == :nested_plain
end

true
