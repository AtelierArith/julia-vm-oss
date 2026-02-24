//! I/O builtin functions for the VM.
//!
//! Print, IOBuffer, and time operations.

use crate::builtins::BuiltinId;
use crate::rng::RngLike;

use super::error::VmError;
use super::stack_ops::StackOps;
use super::util::format_value;
use super::value::{IOKind, IOValue, TupleValue, Value};
use super::Vm;

impl<R: RngLike> Vm<R> {
    /// Execute I/O builtin functions.
    /// Returns `Ok(Some(()))` if handled, `Ok(None)` if not an I/O builtin.
    pub(super) fn execute_builtin_io(
        &mut self,
        builtin: &BuiltinId,
        _argc: usize,
    ) -> Result<Option<()>, VmError> {
        match builtin {
            // =========================================================================
            // I/O Operations
            // =========================================================================
            BuiltinId::Print => {
                let val = self.stack.pop_value()?;
                // Resolve StructRef to Struct for proper formatting
                let resolved = if let Value::StructRef(idx) = &val {
                    if let Some(s) = self.struct_heap.get(*idx) {
                        Value::Struct(s.clone())
                    } else {
                        val
                    }
                } else {
                    val
                };
                let s = format_value(&resolved);
                self.emit_output(&s, false);
            }
            BuiltinId::Println => {
                let val = self.stack.pop_value()?;
                // Resolve StructRef to Struct for proper formatting
                let resolved = if let Value::StructRef(idx) = &val {
                    if let Some(s) = self.struct_heap.get(*idx) {
                        Value::Struct(s.clone())
                    } else {
                        val
                    }
                } else {
                    val
                };
                let s = format_value(&resolved);
                self.emit_output(&s, true);
            }

            // =========================================================================
            // IOBuffer Operations
            // =========================================================================
            BuiltinId::IOBufferNew => {
                // IOBuffer() - create new empty IOBuffer
                self.stack.push(Value::IO(IOValue::buffer_ref()));
            }
            BuiltinId::TakeString => {
                // take!(io) - extract string from IOBuffer and clear it
                let val = self.stack.pop_value()?;
                match val {
                    Value::IO(io_ref) => {
                        let mut io = io_ref.borrow_mut();
                        let result = std::mem::take(&mut io.buffer);
                        self.stack.push(Value::Str(result));
                    }
                    _ => {
                        return Err(VmError::TypeError(
                            "take!/takestring! requires an IOBuffer".to_string(),
                        ))
                    }
                }
            }
            BuiltinId::IOWrite => {
                // write(io, x) - write to IOBuffer (modifies in place)
                let val = self.stack.pop_value()?;
                let io_val = self.stack.pop_value()?;
                match io_val {
                    Value::IO(io_ref) => {
                        // Format the value and append to buffer (in place)
                        let s = format_value(&val);
                        io_ref.borrow_mut().buffer.push_str(&s);
                        // Return the same IORef (now mutated)
                        self.stack.push(Value::IO(io_ref));
                    }
                    _ => {
                        return Err(VmError::TypeError(
                            "write requires an IOBuffer as first argument".to_string(),
                        ))
                    }
                }
            }

            BuiltinId::IOPrint => {
                // print(io, args...) - print multiple args to IOBuffer (modifies in place), returns IO
                // Or print(args...) when first arg is not IO - prints to stdout
                // Stack: [arg1, arg2, ..., argN] (args pushed in order)
                // The _argc in CallBuiltin is the total number of args
                let total_args = _argc;
                if total_args == 0 {
                    // print() with no args - just return nothing
                    self.stack.push(Value::Nothing);
                    return Ok(Some(()));
                }

                // Pop the values (they're in reverse order on stack)
                let mut values = Vec::with_capacity(total_args);
                for _ in 0..total_args {
                    values.push(self.stack.pop_value()?);
                }
                // Reverse to get correct order: [arg1, arg2, ...]
                values.reverse();

                // Check if first value is IO
                match &values[0] {
                    Value::IO(io_ref) => {
                        let print_values = &values[1..];
                        let io_kind = io_ref.borrow().kind.clone();

                        // Special case: if we're inside a sprint call, use emit_output
                        // which will redirect to the sprint buffer.
                        if self.sprint_state.is_some() {
                            for val in print_values {
                                let s = format_value(val);
                                self.emit_output(&s, false);
                            }
                            // Return nothing for sprint context
                            self.stack.push(Value::Nothing);
                        } else if io_kind == IOKind::Stdout {
                            // For stdout, just print to stdout (no IOBuffer to update)
                            for val in print_values {
                                let s = format_value(val);
                                self.emit_output(&s, false);
                            }
                            // Return nothing for stdout (like regular print)
                            self.stack.push(Value::Nothing);
                        } else {
                            // For IOBuffer outside sprint context, write to the buffer in place
                            {
                                let mut io = io_ref.borrow_mut();
                                for val in print_values {
                                    let s = format_value(val);
                                    io.buffer.push_str(&s);
                                }
                            }
                            // Return the same IORef (now mutated)
                            self.stack.push(Value::IO(io_ref.clone()));
                        }
                    }
                    _ => {
                        // First arg is not IO - print all values to stdout
                        for val in &values {
                            let s = format_value(val);
                            self.emit_output(&s, false);
                        }
                        self.stack.push(Value::Nothing);
                    }
                }
            }

            BuiltinId::Displaysize => {
                // displaysize() - return terminal size as (rows, cols)
                // Returns default values since SubsetJuliaVM typically runs
                // in environments without a terminal (iOS, WASM, etc.)
                let rows = Value::I64(24);
                let cols = Value::I64(80);
                self.stack.push(Value::Tuple(TupleValue {
                    elements: vec![rows, cols],
                }));
            }

            // =========================================================================
            // Source File Loading (no-ops)
            // =========================================================================
            BuiltinId::IncludeDependency => {
                // include_dependency(path) - track file dependency for precompilation
                // Since precompilation is not yet implemented, this is a no-op
                // that accepts a path argument and returns nothing
                let _path = self.stack.pop_value()?;
                self.stack.push(Value::Nothing);
            }

            BuiltinId::Precompile => {
                // __precompile__(flag) - control module precompilation
                // Since precompilation is not yet implemented, this is a no-op
                // that accepts a boolean argument and returns nothing
                let _flag = self.stack.pop_value()?;
                self.stack.push(Value::Nothing);
            }

            // =========================================================================
            // Path/Filesystem Operations
            // Note: dirname, basename, joinpath, splitext, splitdir, isabspath, isdirpath
            // are now Pure Julia (base/path.jl) — Issue #2637
            // =========================================================================
            BuiltinId::Normpath => {
                // normpath(path) - normalize path (remove . and ..)
                let path_val = self.stack.pop_value()?;
                let path_str = if let Value::Str(s) = path_val {
                    s
                } else {
                    return Err(VmError::TypeError(format!(
                        "normpath requires a String, got {:?}",
                        path_val
                    )));
                };

                use std::path::{Component, Path, PathBuf};
                let path = Path::new(&path_str);
                let mut normalized = PathBuf::new();
                for component in path.components() {
                    match component {
                        Component::Prefix(p) => normalized.push(p.as_os_str()),
                        Component::RootDir => normalized.push("/"),
                        Component::CurDir => {} // Skip "."
                        Component::ParentDir => {
                            // Pop the last component if possible
                            if !normalized.pop() {
                                normalized.push("..");
                            }
                        }
                        Component::Normal(c) => normalized.push(c),
                    }
                }
                if normalized.as_os_str().is_empty() {
                    normalized.push(".");
                }

                self.stack
                    .push(Value::Str(normalized.to_string_lossy().to_string()));
            }

            BuiltinId::Abspath => {
                // abspath(path) - convert to absolute path
                let path_val = self.stack.pop_value()?;
                let path_str = if let Value::Str(s) = path_val {
                    s
                } else {
                    return Err(VmError::TypeError(format!(
                        "abspath requires a String, got {:?}",
                        path_val
                    )));
                };

                use std::path::Path;
                let path = Path::new(&path_str);
                let abs_path = if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    std::env::current_dir().unwrap_or_default().join(path)
                };

                self.stack
                    .push(Value::Str(abs_path.to_string_lossy().to_string()));
            }

            BuiltinId::Homedir => {
                // homedir() - get home directory
                let home = std::env::var("HOME")
                    .or_else(|_| std::env::var("USERPROFILE"))
                    .unwrap_or_else(|_| "/".to_string());
                self.stack.push(Value::Str(home));
            }

            // =========================================================================
            // Time Operations
            // =========================================================================
            BuiltinId::Sleep => {
                let secs = self.pop_f64_or_i64()?;
                std::thread::sleep(std::time::Duration::from_secs_f64(secs));
                self.stack.push(Value::Nothing);
            }
            BuiltinId::TimeNs => {
                use std::time::{SystemTime, UNIX_EPOCH};
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default();
                self.stack.push(Value::I64(now.as_nanos() as i64));
            }

            // =========================================================================
            // File I/O Operations (read-only)
            // =========================================================================
            BuiltinId::ReadFile => {
                // read(filename, String) - read entire file contents as String
                // Stack: [filename, String_type] -> read file -> push string
                // Pop the type argument (String) - we ignore it since we always return String
                let _type_arg = self.stack.pop_value()?;
                let filename = self.stack.pop_value()?;
                let path = match filename {
                    Value::Str(s) => s,
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "read: expected String for filename, got {:?}",
                            filename
                        )))
                    }
                };
                match std::fs::read_to_string(&path) {
                    Ok(contents) => self.stack.push(Value::Str(contents)),
                    Err(e) => {
                        return Err(VmError::ErrorException(format!(
                            "read: failed to read file '{}': {}",
                            path, e
                        )))
                    }
                }
            }
            BuiltinId::ReadLines => {
                // readlines(filename) - read all lines as Vector{String}
                let filename = self.stack.pop_value()?;
                let path = match filename {
                    Value::Str(s) => s,
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "readlines: expected String for filename, got {:?}",
                            filename
                        )))
                    }
                };
                match std::fs::read_to_string(&path) {
                    Ok(contents) => {
                        let lines: Vec<Value> = contents
                            .lines()
                            .map(|line| Value::Str(line.to_string()))
                            .collect();
                        use super::value::{new_array_ref, ArrayValue};
                        let arr = ArrayValue::any_vector(lines);
                        self.stack.push(Value::Array(new_array_ref(arr)));
                    }
                    Err(e) => {
                        return Err(VmError::ErrorException(format!(
                            "readlines: failed to read file '{}': {}",
                            path, e
                        )))
                    }
                }
            }
            BuiltinId::Readline => {
                // readline(filename) - read first line from file
                let filename = self.stack.pop_value()?;
                let path = match filename {
                    Value::Str(s) => s,
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "readline: expected String for filename, got {:?}",
                            filename
                        )))
                    }
                };
                use std::fs::File;
                use std::io::{BufRead, BufReader};
                match File::open(&path) {
                    Ok(file) => {
                        let reader = BufReader::new(file);
                        match reader.lines().next() {
                            Some(Ok(line)) => {
                                self.stack.push(Value::Str(line));
                            }
                            Some(Err(e)) => {
                                return Err(VmError::ErrorException(format!(
                                    "readline: failed to read line from '{}': {}",
                                    path, e
                                )))
                            }
                            None => {
                                // Empty file returns empty string
                                self.stack.push(Value::Str(String::new()));
                            }
                        }
                    }
                    Err(e) => {
                        return Err(VmError::ErrorException(format!(
                            "readline: failed to open file '{}': {}",
                            path, e
                        )))
                    }
                }
            }
            BuiltinId::Countlines => {
                // countlines(filename) - count lines in file
                let filename = self.stack.pop_value()?;
                let path = match filename {
                    Value::Str(s) => s,
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "countlines: expected String for filename, got {:?}",
                            filename
                        )))
                    }
                };
                use std::fs::File;
                use std::io::{BufRead, BufReader};
                match File::open(&path) {
                    Ok(file) => {
                        let reader = BufReader::new(file);
                        let mut count: i64 = 0;
                        for line_result in reader.lines() {
                            match line_result {
                                Ok(_) => count += 1,
                                Err(e) => {
                                    return Err(VmError::ErrorException(format!(
                                        "countlines: error reading '{}': {}",
                                        path, e
                                    )))
                                }
                            }
                        }
                        // Julia counts the last line even if it doesn't end with newline
                        // BufReader.lines() already handles this correctly
                        self.stack.push(Value::I64(count));
                    }
                    Err(e) => {
                        return Err(VmError::ErrorException(format!(
                            "countlines: failed to open file '{}': {}",
                            path, e
                        )))
                    }
                }
            }
            BuiltinId::Isfile => {
                // isfile(path) - check if path is a regular file
                let path_val = self.stack.pop_value()?;
                let path = match path_val {
                    Value::Str(s) => s,
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "isfile: expected String, got {:?}",
                            path_val
                        )))
                    }
                };
                let result = std::path::Path::new(&path).is_file();
                self.stack.push(Value::Bool(result));
            }
            BuiltinId::Isdir => {
                // isdir(path) - check if path is a directory
                let path_val = self.stack.pop_value()?;
                let path = match path_val {
                    Value::Str(s) => s,
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "isdir: expected String, got {:?}",
                            path_val
                        )))
                    }
                };
                let result = std::path::Path::new(&path).is_dir();
                self.stack.push(Value::Bool(result));
            }
            BuiltinId::Ispath => {
                // ispath(path) - check if path exists (file or directory)
                let path_val = self.stack.pop_value()?;
                let path = match path_val {
                    Value::Str(s) => s,
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "ispath: expected String, got {:?}",
                            path_val
                        )))
                    }
                };
                let result = std::path::Path::new(&path).exists();
                self.stack.push(Value::Bool(result));
            }
            BuiltinId::Filesize => {
                // filesize(path) - get file size in bytes
                let path_val = self.stack.pop_value()?;
                let path = match path_val {
                    Value::Str(s) => s,
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "filesize: expected String, got {:?}",
                            path_val
                        )))
                    }
                };
                match std::fs::metadata(&path) {
                    Ok(meta) => {
                        self.stack.push(Value::I64(meta.len() as i64));
                    }
                    Err(e) => {
                        return Err(VmError::ErrorException(format!(
                            "filesize: failed to get metadata for '{}': {}",
                            path, e
                        )))
                    }
                }
            }

            BuiltinId::Pwd => {
                // pwd() - get current working directory
                match std::env::current_dir() {
                    Ok(path) => {
                        let path_str = path.to_string_lossy().to_string();
                        self.stack.push(Value::Str(path_str));
                    }
                    Err(e) => {
                        return Err(VmError::ErrorException(format!(
                            "pwd: failed to get current directory: {}",
                            e
                        )))
                    }
                }
            }

            BuiltinId::Readdir => {
                // readdir(path) - list directory contents as Vector{String}
                let path_val = self.stack.pop_value()?;
                let path = match &path_val {
                    Value::Str(s) => s.clone(),
                    _ => {
                        return Err(VmError::ErrorException(format!(
                            "readdir: path must be a string, got {:?}",
                            path_val
                        )))
                    }
                };
                match std::fs::read_dir(&path) {
                    Ok(entries) => {
                        let mut names: Vec<Value> = Vec::new();
                        for entry in entries {
                            match entry {
                                Ok(e) => {
                                    let name = e.file_name().to_string_lossy().to_string();
                                    names.push(Value::Str(name));
                                }
                                Err(e) => {
                                    return Err(VmError::ErrorException(format!(
                                        "readdir: error reading entry: {}",
                                        e
                                    )))
                                }
                            }
                        }
                        // Sort the names alphabetically (like Julia does)
                        names.sort_by(|a, b| {
                            if let (Value::Str(sa), Value::Str(sb)) = (a, b) {
                                sa.cmp(sb)
                            } else {
                                std::cmp::Ordering::Equal
                            }
                        });
                        use super::value::{new_array_ref, ArrayValue};
                        let arr = ArrayValue::any_vector(names);
                        self.stack.push(Value::Array(new_array_ref(arr)));
                    }
                    Err(e) => {
                        return Err(VmError::ErrorException(format!(
                            "readdir: failed to read directory '{}': {}",
                            path, e
                        )))
                    }
                }
            }

            BuiltinId::Mkdir => {
                // mkdir(path) - create directory
                let path_val = self.stack.pop_value()?;
                let path = match &path_val {
                    Value::Str(s) => s.clone(),
                    _ => {
                        return Err(VmError::ErrorException(format!(
                            "mkdir: path must be a string, got {:?}",
                            path_val
                        )))
                    }
                };
                match std::fs::create_dir(&path) {
                    Ok(()) => {
                        self.stack.push(Value::Str(path));
                    }
                    Err(e) => {
                        return Err(VmError::ErrorException(format!(
                            "mkdir: failed to create directory '{}': {}",
                            path, e
                        )))
                    }
                }
            }

            BuiltinId::Mkpath => {
                // mkpath(path) - create directory and all parents
                let path_val = self.stack.pop_value()?;
                let path = match &path_val {
                    Value::Str(s) => s.clone(),
                    _ => {
                        return Err(VmError::ErrorException(format!(
                            "mkpath: path must be a string, got {:?}",
                            path_val
                        )))
                    }
                };
                match std::fs::create_dir_all(&path) {
                    Ok(()) => {
                        self.stack.push(Value::Str(path));
                    }
                    Err(e) => {
                        return Err(VmError::ErrorException(format!(
                            "mkpath: failed to create path '{}': {}",
                            path, e
                        )))
                    }
                }
            }

            BuiltinId::Rm => {
                // rm(path) - remove file or empty directory
                let path_val = self.stack.pop_value()?;
                let path = match &path_val {
                    Value::Str(s) => s.clone(),
                    _ => {
                        return Err(VmError::ErrorException(format!(
                            "rm: path must be a string, got {:?}",
                            path_val
                        )))
                    }
                };
                // Check if it's a file or directory
                let metadata = std::fs::metadata(&path);
                match metadata {
                    Ok(m) => {
                        if m.is_dir() {
                            match std::fs::remove_dir(&path) {
                                Ok(()) => {
                                    self.stack.push(Value::Nothing);
                                }
                                Err(e) => {
                                    return Err(VmError::ErrorException(format!(
                                        "rm: failed to remove directory '{}': {}",
                                        path, e
                                    )))
                                }
                            }
                        } else {
                            match std::fs::remove_file(&path) {
                                Ok(()) => {
                                    self.stack.push(Value::Nothing);
                                }
                                Err(e) => {
                                    return Err(VmError::ErrorException(format!(
                                        "rm: failed to remove file '{}': {}",
                                        path, e
                                    )))
                                }
                            }
                        }
                    }
                    Err(e) => {
                        return Err(VmError::ErrorException(format!(
                            "rm: path '{}' not found: {}",
                            path, e
                        )))
                    }
                }
            }

            BuiltinId::Tempdir => {
                // tempdir() - get system temp directory
                let temp_dir = std::env::temp_dir();
                let path_str = temp_dir.to_string_lossy().to_string();
                self.stack.push(Value::Str(path_str));
            }

            BuiltinId::Tempname => {
                // tempname() - generate unique temp filename
                let temp_dir = std::env::temp_dir();
                // Generate a random suffix using timestamp and a counter
                use std::time::{SystemTime, UNIX_EPOCH};
                let timestamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                // Use a simple random-like value from timestamp
                let random_part = format!("{:x}", timestamp);
                let filename = format!(
                    "jl_{}",
                    &random_part[random_part.len().saturating_sub(12)..]
                );
                let path = temp_dir.join(&filename);
                let path_str = path.to_string_lossy().to_string();
                self.stack.push(Value::Str(path_str));
            }

            BuiltinId::Touch => {
                // touch(path) - create empty file or update mtime
                let path_val = self.stack.pop_value()?;
                let path = match &path_val {
                    Value::Str(s) => s.clone(),
                    _ => {
                        return Err(VmError::ErrorException(format!(
                            "touch: path must be a string, got {:?}",
                            path_val
                        )))
                    }
                };
                // If file exists, open with append to update mtime
                // If not exists, create empty file
                use std::fs::OpenOptions;
                match OpenOptions::new()
                    .create(true)
                    .append(true) // append mode updates mtime on open
                    .open(&path)
                {
                    Ok(_file) => {
                        // File is created or mtime updated by opening
                        self.stack.push(Value::Str(path));
                    }
                    Err(e) => {
                        return Err(VmError::ErrorException(format!(
                            "touch: failed to touch file '{}': {}",
                            path, e
                        )))
                    }
                }
            }

            BuiltinId::Cd => {
                // cd(path) - change current directory
                let path_val = self.stack.pop_value()?;
                let path = match &path_val {
                    Value::Str(s) => s.clone(),
                    _ => {
                        return Err(VmError::ErrorException(format!(
                            "cd: path must be a string, got {:?}",
                            path_val
                        )))
                    }
                };
                match std::env::set_current_dir(&path) {
                    Ok(()) => {
                        // Return the new working directory
                        match std::env::current_dir() {
                            Ok(cwd) => {
                                self.stack
                                    .push(Value::Str(cwd.to_string_lossy().to_string()));
                            }
                            Err(_) => {
                                self.stack.push(Value::Str(path));
                            }
                        }
                    }
                    Err(e) => {
                        return Err(VmError::ErrorException(format!(
                            "cd: failed to change directory to '{}': {}",
                            path, e
                        )))
                    }
                }
            }

            BuiltinId::Islink => {
                // islink(path) - check if path is a symbolic link
                let path_val = self.stack.pop_value()?;
                let path = match &path_val {
                    Value::Str(s) => s.clone(),
                    _ => {
                        return Err(VmError::ErrorException(format!(
                            "islink: path must be a string, got {:?}",
                            path_val
                        )))
                    }
                };
                // Use symlink_metadata to get info about the link itself, not the target
                let is_link = std::fs::symlink_metadata(&path)
                    .map(|m| m.file_type().is_symlink())
                    .unwrap_or(false);
                self.stack.push(Value::Bool(is_link));
            }

            BuiltinId::Cp => {
                // cp(src, dst) - copy file
                let dst_val = self.stack.pop_value()?;
                let src_val = self.stack.pop_value()?;
                let src = match &src_val {
                    Value::Str(s) => s.clone(),
                    _ => {
                        return Err(VmError::ErrorException(format!(
                            "cp: source path must be a string, got {:?}",
                            src_val
                        )))
                    }
                };
                let dst = match &dst_val {
                    Value::Str(s) => s.clone(),
                    _ => {
                        return Err(VmError::ErrorException(format!(
                            "cp: destination path must be a string, got {:?}",
                            dst_val
                        )))
                    }
                };
                match std::fs::copy(&src, &dst) {
                    Ok(_bytes_copied) => {
                        self.stack.push(Value::Str(dst));
                    }
                    Err(e) => {
                        return Err(VmError::ErrorException(format!(
                            "cp: failed to copy '{}' to '{}': {}",
                            src, dst, e
                        )))
                    }
                }
            }

            BuiltinId::Mv => {
                // mv(src, dst) - move/rename file
                let dst_val = self.stack.pop_value()?;
                let src_val = self.stack.pop_value()?;
                let src = match &src_val {
                    Value::Str(s) => s.clone(),
                    _ => {
                        return Err(VmError::ErrorException(format!(
                            "mv: source path must be a string, got {:?}",
                            src_val
                        )))
                    }
                };
                let dst = match &dst_val {
                    Value::Str(s) => s.clone(),
                    _ => {
                        return Err(VmError::ErrorException(format!(
                            "mv: destination path must be a string, got {:?}",
                            dst_val
                        )))
                    }
                };
                match std::fs::rename(&src, &dst) {
                    Ok(()) => {
                        self.stack.push(Value::Str(dst));
                    }
                    Err(e) => {
                        return Err(VmError::ErrorException(format!(
                            "mv: failed to move '{}' to '{}': {}",
                            src, dst, e
                        )))
                    }
                }
            }

            BuiltinId::Mtime => {
                // mtime(path) - get modification time as Unix timestamp (Float64)
                let path_val = self.stack.pop_value()?;
                let path = match &path_val {
                    Value::Str(s) => s.clone(),
                    _ => {
                        return Err(VmError::ErrorException(format!(
                            "mtime: path must be a string, got {:?}",
                            path_val
                        )))
                    }
                };
                use std::time::UNIX_EPOCH;
                match std::fs::metadata(&path) {
                    Ok(metadata) => {
                        match metadata.modified() {
                            Ok(modified_time) => {
                                let duration =
                                    modified_time.duration_since(UNIX_EPOCH).unwrap_or_default();
                                // Julia returns seconds as Float64
                                let secs = duration.as_secs_f64();
                                self.stack.push(Value::F64(secs));
                            }
                            Err(e) => {
                                return Err(VmError::ErrorException(format!(
                                    "mtime: failed to get modification time for '{}': {}",
                                    path, e
                                )))
                            }
                        }
                    }
                    Err(e) => {
                        return Err(VmError::ErrorException(format!(
                            "mtime: failed to get metadata for '{}': {}",
                            path, e
                        )))
                    }
                }
            }

            // =========================================================================
            // File Handle Operations
            // =========================================================================
            BuiltinId::Open => {
                // open(filename) - open for reading
                // open(filename, mode) - open with mode
                let (path, mode) = if _argc == 2 {
                    let mode_val = self.stack.pop_value()?;
                    let path_val = self.stack.pop_value()?;
                    let path = match path_val {
                        Value::Str(s) => s,
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "open: expected String for filename, got {:?}",
                                path_val
                            )))
                        }
                    };
                    let mode = match mode_val {
                        Value::Str(s) => s,
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "open: expected String for mode, got {:?}",
                                mode_val
                            )))
                        }
                    };
                    (path, mode)
                } else {
                    let path_val = self.stack.pop_value()?;
                    let path = match path_val {
                        Value::Str(s) => s,
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "open: expected String for filename, got {:?}",
                                path_val
                            )))
                        }
                    };
                    (path, "r".to_string())
                };

                // Parse mode string (like Julia's fopen)
                let (readable, writable, create, truncate, append) = match mode.as_str() {
                    "r" => (true, false, false, false, false),
                    "r+" => (true, true, false, false, false),
                    "w" => (false, true, true, true, false),
                    "w+" => (true, true, true, true, false),
                    "a" => (false, true, true, false, true),
                    "a+" => (true, true, true, false, true),
                    _ => {
                        return Err(VmError::ErrorException(format!(
                            "open: invalid mode '{}'. Valid modes are: r, r+, w, w+, a, a+",
                            mode
                        )))
                    }
                };

                use std::fs::OpenOptions;
                let file_result = OpenOptions::new()
                    .read(readable)
                    .write(writable)
                    .create(create)
                    .truncate(truncate)
                    .append(append)
                    .open(&path);

                match file_result {
                    Ok(file) => {
                        let io_val = IOValue::file_from(file, path, readable, writable);
                        self.stack.push(Value::IO(io_val.into_ref()));
                    }
                    Err(e) => {
                        return Err(VmError::ErrorException(format!(
                            "open: failed to open file '{}': {}",
                            path, e
                        )))
                    }
                }
            }

            BuiltinId::Close => {
                // close(io) - close IO stream
                let io_val = self.stack.pop_value()?;
                match io_val {
                    Value::IO(io_ref) => {
                        let io = io_ref.borrow();
                        if let Some(ref handle) = io.file_handle {
                            handle.borrow_mut().close();
                        }
                        // For other IO types, close is a no-op
                        self.stack.push(Value::Nothing);
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "close: expected IO stream, got {:?}",
                            io_val
                        )))
                    }
                }
            }

            BuiltinId::Eof => {
                // eof(io) - check if at end of file
                let io_val = self.stack.pop_value()?;
                match io_val {
                    Value::IO(io_ref) => {
                        let io = io_ref.borrow();
                        let at_eof = if let Some(ref handle) = io.file_handle {
                            match handle.borrow_mut().eof() {
                                Ok(eof) => eof,
                                Err(e) => {
                                    return Err(VmError::ErrorException(format!(
                                        "eof: error checking EOF: {}",
                                        e
                                    )))
                                }
                            }
                        } else {
                            // For IOBuffer, check if buffer is empty (at "EOF")
                            match io.kind {
                                IOKind::Buffer => io.buffer.is_empty(),
                                _ => false, // stdout/stderr/stdin are never at EOF
                            }
                        };
                        self.stack.push(Value::Bool(at_eof));
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "eof: expected IO stream, got {:?}",
                            io_val
                        )))
                    }
                }
            }

            BuiltinId::Isopen => {
                // isopen(io) - check if IO stream is open
                let io_val = self.stack.pop_value()?;
                match io_val {
                    Value::IO(io_ref) => {
                        let io = io_ref.borrow();
                        let is_open = io.is_open();
                        self.stack.push(Value::Bool(is_open));
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "isopen: expected IO stream, got {:?}",
                            io_val
                        )))
                    }
                }
            }

            BuiltinId::ReadlineIo => {
                // readline(io) - read line from IO stream
                let io_val = self.stack.pop_value()?;
                match io_val {
                    Value::IO(io_ref) => {
                        // Clone the file handle to avoid borrow issues
                        let handle_opt = {
                            let io = io_ref.borrow();
                            io.file_handle.clone()
                        };

                        if let Some(handle) = handle_opt {
                            match handle.borrow_mut().readline() {
                                Ok(Some(line)) => {
                                    self.stack.push(Value::Str(line));
                                }
                                Ok(None) => {
                                    // EOF - return empty string
                                    self.stack.push(Value::Str(String::new()));
                                }
                                Err(e) => {
                                    return Err(VmError::ErrorException(format!(
                                        "readline: error reading line: {}",
                                        e
                                    )))
                                }
                            }
                        } else {
                            return Err(VmError::TypeError(
                                "readline: IO stream is not a file".to_string(),
                            ));
                        }
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "readline: expected IO stream, got {:?}",
                            io_val
                        )))
                    }
                }
            }

            _ => return Ok(None),
        }
        Ok(Some(()))
    }
}
