//! IO stream types for print/show operations.

use std::cell::RefCell;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::rc::Rc;

/// IO reference type - provides interior mutability for IO buffers.
/// This allows IOBuffer to be modified in place, matching Julia's semantics.
pub type IORef = Rc<RefCell<IOValue>>;

/// IO stream kind for print/show operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IOKind {
    /// Standard output (default)
    Stdout,
    /// Standard error
    Stderr,
    /// Standard input
    Stdin,
    /// In-memory buffer for capturing output
    Buffer,
    /// /dev/null (discards all output)
    Devnull,
    /// File handle for actual file I/O
    File,
}

/// Shared file handle type for file I/O
pub type FileHandle = Rc<RefCell<FileState>>;

/// File state containing the actual file handle and metadata
#[derive(Debug)]
pub struct FileState {
    /// The underlying file handle (None if closed)
    pub file: Option<File>,
    /// File path
    pub path: String,
    /// Whether the file is readable
    pub readable: bool,
    /// Whether the file is writable
    pub writable: bool,
    /// Buffered reader for line-by-line reading
    reader: Option<BufReader<File>>,
    /// Read position in buffer
    read_buffer: String,
    /// Current line for readline operations
    current_line: String,
    /// Whether we've reached EOF
    at_eof: bool,
}

impl FileState {
    /// Create a new file state
    pub fn new(file: File, path: String, readable: bool, writable: bool) -> Self {
        Self {
            file: Some(file),
            path,
            readable,
            writable,
            reader: None,
            read_buffer: String::new(),
            current_line: String::new(),
            at_eof: false,
        }
    }

    /// Check if the file is open
    pub fn is_open(&self) -> bool {
        self.file.is_some()
    }

    /// Close the file
    pub fn close(&mut self) {
        self.file = None;
        self.reader = None;
    }

    /// Check if at end of file
    pub fn eof(&mut self) -> std::io::Result<bool> {
        if self.at_eof {
            return Ok(true);
        }
        if let Some(ref mut file) = self.file {
            // Check if we're at EOF by trying to read
            let current_pos = file.stream_position()?;
            let end_pos = file.seek(SeekFrom::End(0))?;
            file.seek(SeekFrom::Start(current_pos))?;
            self.at_eof = current_pos >= end_pos;
            Ok(self.at_eof)
        } else {
            Ok(true) // Closed file is considered at EOF
        }
    }

    /// Read entire file contents as String
    pub fn read_string(&mut self) -> std::io::Result<String> {
        if let Some(ref mut file) = self.file {
            let mut contents = String::new();
            file.read_to_string(&mut contents)?;
            self.at_eof = true;
            Ok(contents)
        } else {
            Err(std::io::Error::other("file is closed"))
        }
    }

    /// Read a single line from the file
    pub fn readline(&mut self) -> std::io::Result<Option<String>> {
        if self.at_eof {
            return Ok(None);
        }

        // Take ownership of the file temporarily to create a BufReader
        if let Some(file) = self.file.take() {
            let mut reader = BufReader::new(file);
            let mut line = String::new();
            let bytes_read = reader.read_line(&mut line)?;

            // Put the file back
            self.file = Some(reader.into_inner());

            if bytes_read == 0 {
                self.at_eof = true;
                Ok(None)
            } else {
                // Remove trailing newline if present
                if line.ends_with('\n') {
                    line.pop();
                    if line.ends_with('\r') {
                        line.pop();
                    }
                }
                Ok(Some(line))
            }
        } else {
            Err(std::io::Error::other("file is closed"))
        }
    }

    /// Write string to file
    pub fn write_str(&mut self, s: &str) -> std::io::Result<usize> {
        if let Some(ref mut file) = self.file {
            file.write_all(s.as_bytes())?;
            Ok(s.len())
        } else {
            Err(std::io::Error::other("file is closed"))
        }
    }
}

impl Clone for FileState {
    fn clone(&self) -> Self {
        // FileState can't truly be cloned since File handles can't be cloned.
        // This creates a "closed" clone - actual file sharing is done via Rc<RefCell<>>
        Self {
            file: None, // Can't clone the actual file handle
            path: self.path.clone(),
            readable: self.readable,
            writable: self.writable,
            reader: None,
            read_buffer: self.read_buffer.clone(),
            current_line: self.current_line.clone(),
            at_eof: self.at_eof,
        }
    }
}

/// IO stream value
#[derive(Debug, Clone)]
pub struct IOValue {
    /// Kind of IO stream
    pub kind: IOKind,
    /// Buffer content (only used for Buffer kind)
    pub buffer: String,
    /// File handle (only used for File kind)
    pub file_handle: Option<FileHandle>,
}

impl IOValue {
    /// Create a new stdout IO value
    pub fn stdout() -> Self {
        Self {
            kind: IOKind::Stdout,
            buffer: String::new(),
            file_handle: None,
        }
    }

    /// Create a new stderr IO value
    pub fn stderr() -> Self {
        Self {
            kind: IOKind::Stderr,
            buffer: String::new(),
            file_handle: None,
        }
    }

    /// Create a new stdin IO value
    pub fn stdin() -> Self {
        Self {
            kind: IOKind::Stdin,
            buffer: String::new(),
            file_handle: None,
        }
    }

    /// Create a new buffer IO value
    pub fn buffer() -> Self {
        Self {
            kind: IOKind::Buffer,
            buffer: String::new(),
            file_handle: None,
        }
    }

    /// Create a /dev/null IO value (discards all output)
    pub fn devnull() -> Self {
        Self {
            kind: IOKind::Devnull,
            buffer: String::new(),
            file_handle: None,
        }
    }

    /// Create a new file IO value
    pub fn file(handle: FileHandle) -> Self {
        Self {
            kind: IOKind::File,
            buffer: String::new(),
            file_handle: Some(handle),
        }
    }

    /// Create a file handle from a File
    pub fn file_from(file: File, path: String, readable: bool, writable: bool) -> Self {
        let state = FileState::new(file, path, readable, writable);
        let handle = Rc::new(RefCell::new(state));
        Self::file(handle)
    }

    /// Check if this is stdout
    pub fn is_stdout(&self) -> bool {
        self.kind == IOKind::Stdout
    }

    /// Check if this is stderr
    pub fn is_stderr(&self) -> bool {
        self.kind == IOKind::Stderr
    }

    /// Check if this is stdin
    pub fn is_stdin(&self) -> bool {
        self.kind == IOKind::Stdin
    }

    /// Check if this is devnull
    pub fn is_devnull(&self) -> bool {
        self.kind == IOKind::Devnull
    }

    /// Check if this is a file
    pub fn is_file(&self) -> bool {
        self.kind == IOKind::File
    }

    /// Check if this is a buffer
    pub fn is_buffer(&self) -> bool {
        self.kind == IOKind::Buffer
    }

    /// Check if the IO stream is open
    pub fn is_open(&self) -> bool {
        match self.kind {
            IOKind::File => {
                if let Some(ref handle) = self.file_handle {
                    handle.borrow().is_open()
                } else {
                    false
                }
            }
            _ => true, // stdout/stderr/stdin/buffer are always "open"
        }
    }

    /// Create an IORef (Rc<RefCell<IOValue>>) from this IOValue
    pub fn into_ref(self) -> IORef {
        Rc::new(RefCell::new(self))
    }

    /// Create a new stdout IO reference
    pub fn stdout_ref() -> IORef {
        Self::stdout().into_ref()
    }

    /// Create a new stderr IO reference
    pub fn stderr_ref() -> IORef {
        Self::stderr().into_ref()
    }

    /// Create a new stdin IO reference
    pub fn stdin_ref() -> IORef {
        Self::stdin().into_ref()
    }

    /// Create a new buffer IO reference
    pub fn buffer_ref() -> IORef {
        Self::buffer().into_ref()
    }

    /// Create a /dev/null IO reference
    pub fn devnull_ref() -> IORef {
        Self::devnull().into_ref()
    }

    /// Create a file IO reference
    pub fn file_ref(file: File, path: String, readable: bool, writable: bool) -> IORef {
        Self::file_from(file, path, readable, writable).into_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── constructors set correct kind ─────────────────────────────────────────

    #[test]
    fn test_stdout_constructor_sets_kind() {
        let io = IOValue::stdout();
        assert_eq!(io.kind, IOKind::Stdout);
        assert!(io.buffer.is_empty());
        assert!(io.file_handle.is_none());
    }

    #[test]
    fn test_stderr_constructor_sets_kind() {
        assert_eq!(IOValue::stderr().kind, IOKind::Stderr);
    }

    #[test]
    fn test_stdin_constructor_sets_kind() {
        assert_eq!(IOValue::stdin().kind, IOKind::Stdin);
    }

    #[test]
    fn test_buffer_constructor_sets_kind() {
        let io = IOValue::buffer();
        assert_eq!(io.kind, IOKind::Buffer);
        assert!(io.buffer.is_empty(), "buffer should start empty");
    }

    #[test]
    fn test_devnull_constructor_sets_kind() {
        assert_eq!(IOValue::devnull().kind, IOKind::Devnull);
    }

    // ── is_* predicates are mutually exclusive ────────────────────────────────

    #[test]
    fn test_is_stdout_true_only_for_stdout() {
        assert!(IOValue::stdout().is_stdout());
        assert!(!IOValue::stderr().is_stdout());
        assert!(!IOValue::stdin().is_stdout());
        assert!(!IOValue::buffer().is_stdout());
        assert!(!IOValue::devnull().is_stdout());
    }

    #[test]
    fn test_is_stderr_true_only_for_stderr() {
        assert!(IOValue::stderr().is_stderr());
        assert!(!IOValue::stdout().is_stderr());
    }

    #[test]
    fn test_is_stdin_true_only_for_stdin() {
        assert!(IOValue::stdin().is_stdin());
        assert!(!IOValue::stdout().is_stdin());
    }

    #[test]
    fn test_is_devnull_true_only_for_devnull() {
        assert!(IOValue::devnull().is_devnull());
        assert!(!IOValue::stdout().is_devnull());
        assert!(!IOValue::buffer().is_devnull());
    }

    #[test]
    fn test_is_file_false_for_non_file_kinds() {
        assert!(!IOValue::stdout().is_file());
        assert!(!IOValue::stderr().is_file());
        assert!(!IOValue::buffer().is_file());
        assert!(!IOValue::devnull().is_file());
    }

    // ── is_open for non-file kinds ────────────────────────────────────────────

    #[test]
    fn test_is_open_stdout_always_true() {
        assert!(IOValue::stdout().is_open(), "stdout is always open");
    }

    #[test]
    fn test_is_open_buffer_always_true() {
        assert!(IOValue::buffer().is_open(), "buffer is always open");
    }

    #[test]
    fn test_is_open_devnull_always_true() {
        assert!(IOValue::devnull().is_open(), "devnull is always open");
    }

    // ── into_ref wraps correctly ──────────────────────────────────────────────

    #[test]
    fn test_into_ref_wraps_in_rc_refcell() {
        let io = IOValue::buffer();
        let ioref = io.into_ref();
        // Borrow to verify kind is preserved
        assert_eq!(ioref.borrow().kind, IOKind::Buffer);
    }

    #[test]
    fn test_stdout_ref_factory() {
        let ioref = IOValue::stdout_ref();
        assert!(ioref.borrow().is_stdout());
    }

    #[test]
    fn test_buffer_ref_factory() {
        let ioref = IOValue::buffer_ref();
        assert!(ioref.borrow().kind == IOKind::Buffer);
    }
}
