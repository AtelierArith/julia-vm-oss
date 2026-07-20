//! Shared module-name classification below the compiler/VM boundary.

const MAIN_MODULE: &str = "Main";
const BASE_MODULE: &str = "Base";
const CORE_MODULE: &str = "Core";
const SYS_MODULE: &str = "Sys";
const META_MODULE: &str = "Meta";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuiltinModule {
    Main,
    Base,
    Core,
    Sys,
    Meta,
    Other,
}

pub fn classify_builtin_module(name: &str) -> BuiltinModule {
    match name {
        MAIN_MODULE => BuiltinModule::Main,
        BASE_MODULE => BuiltinModule::Base,
        CORE_MODULE => BuiltinModule::Core,
        SYS_MODULE => BuiltinModule::Sys,
        META_MODULE => BuiltinModule::Meta,
        _ => BuiltinModule::Other,
    }
}

pub fn is_language_root(name: &str) -> bool {
    matches!(
        classify_builtin_module(name),
        BuiltinModule::Main | BuiltinModule::Base | BuiltinModule::Core
    )
}

pub fn is_base(name: &str) -> bool {
    classify_builtin_module(name) == BuiltinModule::Base
}

pub fn is_builtin_literal_root(name: &str) -> bool {
    classify_builtin_module(name) != BuiltinModule::Other
}

pub fn is_language_root_path(path: &str) -> bool {
    is_language_root(path.split_once('.').map_or(path, |(root, _)| root))
}

pub fn is_root_module_name(name: &str) -> bool {
    is_builtin_literal_root(name)
        || matches!(
            name,
            "LinearAlgebra"
                | "Statistics"
                | "Random"
                | "Dates"
                | "Printf"
                | "Test"
                | "SparseArrays"
                | "Distributed"
                | "SharedArrays"
                | "Serialization"
                | "REPL"
                | "InteractiveUtils"
                | "Pkg"
                | "Markdown"
                | "UUIDs"
                | "Sockets"
                | "DelimitedFiles"
                | "FileWatching"
        )
}
