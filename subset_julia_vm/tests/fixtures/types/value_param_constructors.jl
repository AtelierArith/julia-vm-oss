using Test
import Base: HasShape

@testset "zero-field value-parameter constructors (Issue #4084)" begin
    val_one = Val{1}()
    @test typeof(val_one) === Val{1}
    @test val_one isa Val{1}

    shape_one = HasShape{1}()
    @test typeof(shape_one) === HasShape{1}
    @test shape_one isa HasShape{1}

    base_shape_one = Base.HasShape{1}()
    @test typeof(base_shape_one) === Base.HasShape{1}
    @test base_shape_one isa Base.HasShape{1}

    base_shape_two = Base.HasShape{2}()
    @test typeof(base_shape_two) === Base.HasShape{2}
    @test base_shape_two isa Base.HasShape{2}
end

true
