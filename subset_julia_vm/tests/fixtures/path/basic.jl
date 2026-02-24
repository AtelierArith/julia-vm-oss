# Test path manipulation functions: basename, dirname, splitdir, splitext, joinpath, isabspath, isdirpath
# Based on Julia's base/path.jl
#
# Note: Uses isequal for string comparison since == operator for String is not yet implemented

using Test

@testset "path manipulation functions" begin
    # dirname - get directory part of path
    @test isequal(dirname("/home/user/file.txt"), "/home/user")
    @test isequal(dirname("/home/user/"), "/home/user")
    @test isequal(dirname("/home/user"), "/home")
    @test isequal(dirname("/home"), "/")
    @test isequal(dirname("file.txt"), "")

    # basename - get file name part of path
    @test isequal(basename("/home/user/file.txt"), "file.txt")
    @test isequal(basename("/home/user/"), "")
    @test isequal(basename("/home/user"), "user")
    @test isequal(basename("file.txt"), "file.txt")

    # splitdir - split path into (directory, file)
    d1, f1 = splitdir("/home/user/file.txt")
    @test isequal(d1, "/home/user")
    @test isequal(f1, "file.txt")

    d2, f2 = splitdir("/home/user/")
    @test isequal(d2, "/home/user")
    @test isequal(f2, "")

    d3, f3 = splitdir("/home/user")
    @test isequal(d3, "/home")
    @test isequal(f3, "user")

    d4, f4 = splitdir("/home")
    @test isequal(d4, "/")
    @test isequal(f4, "home")

    d5, f5 = splitdir("file.txt")
    @test isequal(d5, "")
    @test isequal(f5, "file.txt")

    # splitext - split path into (path without extension, extension)
    p1, e1 = splitext("/home/user/file.txt")
    @test isequal(p1, "/home/user/file")
    @test isequal(e1, ".txt")

    p2, e2 = splitext("/home/user/file.tar.gz")
    @test isequal(p2, "/home/user/file.tar")
    @test isequal(e2, ".gz")

    p3, e3 = splitext("/home/user/file")
    @test isequal(p3, "/home/user/file")
    @test isequal(e3, "")

    p4, e4 = splitext("file.jl")
    @test isequal(p4, "file")
    @test isequal(e4, ".jl")

    p5, e5 = splitext(".hidden")
    @test isequal(p5, ".hidden")
    @test isequal(e5, "")

    # joinpath - join path components
    @test isequal(joinpath("a", "b"), "a/b")
    @test isequal(joinpath("a/", "b"), "a/b")
    @test isequal(joinpath("a", "/b"), "/b")
    @test isequal(joinpath("", "b"), "b")
    @test isequal(joinpath("a", "b", "c"), "a/b/c")
    @test isequal(joinpath("/home", "user", "file.txt"), "/home/user/file.txt")

    # isabspath - check if path is absolute
    @test isabspath("/home/user") == true
    @test isabspath("/") == true
    @test isabspath("home/user") == false
    @test isabspath("") == false

    # isdirpath - check if path ends with separator
    @test isdirpath("/home/user/") == true
    @test isdirpath("/") == true
    @test isdirpath("/home/user") == false
    @test isdirpath("") == false
end

true
