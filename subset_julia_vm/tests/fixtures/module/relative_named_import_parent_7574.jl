using Test

module RelativeParentImport7574
x() = 1

module Child
import ..RelativeParentImport7574: x

println(x())
child_value() = x() + 41
end
end

module RelativeSiblingImport7574
module Source
source_value() = 99
end

module Sink
import ..Source: source_value

println(source_value())
sink_value() = source_value()
end
end

@test RelativeParentImport7574.Child.child_value() == 42
@test RelativeSiblingImport7574.Sink.sink_value() == 99

true
