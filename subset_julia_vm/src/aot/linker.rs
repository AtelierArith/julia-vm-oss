//! System linker planning for Cranelift object output.
//!
//! The Cranelift backend emits relocatable objects directly. This module keeps
//! linker discovery and platform-specific argument ordering separate from the
//! CLI so `--emit-binary`, library output, and future package drivers can reuse
//! the same boundary (Issue #7089).

use std::env;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkerKind {
    /// C compiler driver (`cc`, `clang`, `gcc`). This is the preferred Unix path
    /// because the driver supplies startup objects and the platform C runtime.
    CcDriver,
    /// Direct Unix linker (`ld`, `ld.lld`). This path is primarily for explicit
    /// lld integration and tests; callers may need extra startup-object args.
    UnixLd,
    /// MSVC-compatible linker (`link.exe`, `lld-link`).
    MsvcLink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkTargetFamily {
    Unix,
    Darwin,
    WindowsMsvc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkOutputKind {
    Executable,
    SharedLibrary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkerConfig {
    pub object_files: Vec<PathBuf>,
    pub output_path: PathBuf,
    pub target_triple: Option<String>,
    pub linker: Option<PathBuf>,
    pub runtime_libraries: Vec<PathBuf>,
    pub extra_args: Vec<OsString>,
    pub output_kind: LinkOutputKind,
}

impl LinkerConfig {
    pub fn new(output_path: impl Into<PathBuf>) -> Self {
        Self {
            object_files: Vec::new(),
            output_path: output_path.into(),
            target_triple: None,
            linker: None,
            runtime_libraries: Vec::new(),
            extra_args: Vec::new(),
            output_kind: LinkOutputKind::Executable,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkerInvocation {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub kind: LinkerKind,
    pub target_family: LinkTargetFamily,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkerError {
    NoObjectFiles,
    LinkerNotFound {
        target_triple: Option<String>,
        candidates: Vec<&'static str>,
    },
    LinkFailed {
        program: PathBuf,
        status: String,
    },
    LinkerLaunchFailed {
        program: PathBuf,
        message: String,
    },
}

impl fmt::Display for LinkerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoObjectFiles => write!(f, "system linker requires at least one object file"),
            Self::LinkerNotFound {
                target_triple,
                candidates,
            } => {
                write!(
                    f,
                    "no system linker found for target {} (tried: {})",
                    target_triple.as_deref().unwrap_or("<host>"),
                    candidates.join(", ")
                )
            }
            Self::LinkFailed { program, status } => {
                write!(f, "{} failed with status {}", program.display(), status)
            }
            Self::LinkerLaunchFailed { program, message } => {
                write!(f, "failed to run {}: {}", program.display(), message)
            }
        }
    }
}

impl std::error::Error for LinkerError {}

pub fn plan_system_link(config: &LinkerConfig) -> Result<LinkerInvocation, LinkerError> {
    if config.object_files.is_empty() {
        return Err(LinkerError::NoObjectFiles);
    }

    let target_family = target_family(config.target_triple.as_deref());
    let (program, kind) = resolve_linker(
        config.linker.as_deref(),
        target_family,
        config.target_triple.clone(),
    )?;
    let args = linker_args(&program, kind, target_family, config);

    Ok(LinkerInvocation {
        program,
        args,
        kind,
        target_family,
    })
}

pub fn link_objects(config: &LinkerConfig) -> Result<(), LinkerError> {
    let invocation = plan_system_link(config)?;
    let status = Command::new(&invocation.program)
        .args(&invocation.args)
        .status()
        .map_err(|e| LinkerError::LinkerLaunchFailed {
            program: invocation.program.clone(),
            message: e.to_string(),
        })?;
    if !status.success() {
        return Err(LinkerError::LinkFailed {
            program: invocation.program,
            status: status.to_string(),
        });
    }
    Ok(())
}

fn target_family(target_triple: Option<&str>) -> LinkTargetFamily {
    let Some(target) = target_triple else {
        return host_target_family();
    };
    if target.contains("windows-msvc") {
        LinkTargetFamily::WindowsMsvc
    } else if target.contains("apple") || target.contains("darwin") {
        LinkTargetFamily::Darwin
    } else {
        LinkTargetFamily::Unix
    }
}

fn host_target_family() -> LinkTargetFamily {
    if cfg!(target_os = "windows") {
        LinkTargetFamily::WindowsMsvc
    } else if cfg!(target_os = "macos") || cfg!(target_os = "ios") {
        LinkTargetFamily::Darwin
    } else {
        LinkTargetFamily::Unix
    }
}

fn resolve_linker(
    explicit: Option<&Path>,
    target_family: LinkTargetFamily,
    target_triple: Option<String>,
) -> Result<(PathBuf, LinkerKind), LinkerError> {
    if let Some(linker) = explicit {
        return Ok((linker.to_path_buf(), classify_linker(linker, target_family)));
    }

    let candidates = linker_candidates(target_family);
    for candidate in &candidates {
        if let Some(path) = find_program(candidate) {
            return Ok((path, classify_linker(Path::new(candidate), target_family)));
        }
    }
    Err(LinkerError::LinkerNotFound {
        target_triple,
        candidates,
    })
}

fn linker_candidates(target_family: LinkTargetFamily) -> Vec<&'static str> {
    match target_family {
        LinkTargetFamily::WindowsMsvc => vec!["link.exe", "lld-link"],
        LinkTargetFamily::Darwin => vec!["cc", "clang", "ld64.lld"],
        LinkTargetFamily::Unix => vec!["cc", "clang", "gcc", "ld.lld"],
    }
}

fn find_program(name: &str) -> Option<PathBuf> {
    let path = Path::new(name);
    if path.components().count() > 1 {
        return path.exists().then(|| path.to_path_buf());
    }
    let paths = env::var_os("PATH")?;
    env::split_paths(&paths)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

fn classify_linker(path: &Path, target_family: LinkTargetFamily) -> LinkerKind {
    let name = path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if name == "link.exe" || name == "link" || name.contains("lld-link") {
        LinkerKind::MsvcLink
    } else if name.starts_with("ld") || name.contains("ld.lld") || name.contains("ld64.lld") {
        LinkerKind::UnixLd
    } else if target_family == LinkTargetFamily::WindowsMsvc {
        LinkerKind::MsvcLink
    } else {
        LinkerKind::CcDriver
    }
}

fn linker_args(
    program: &Path,
    kind: LinkerKind,
    target_family: LinkTargetFamily,
    config: &LinkerConfig,
) -> Vec<OsString> {
    match kind {
        LinkerKind::CcDriver => cc_driver_args(program, target_family, config),
        LinkerKind::UnixLd => unix_ld_args(target_family, config),
        LinkerKind::MsvcLink => msvc_link_args(config),
    }
}

fn cc_driver_args(
    program: &Path,
    target_family: LinkTargetFamily,
    config: &LinkerConfig,
) -> Vec<OsString> {
    let mut args = Vec::new();
    if is_clang_like(program) {
        if let Some(target) = &config.target_triple {
            args.push(format!("--target={target}").into());
        }
    }
    match config.output_kind {
        LinkOutputKind::Executable => {}
        LinkOutputKind::SharedLibrary if target_family == LinkTargetFamily::Darwin => {
            args.push("-dynamiclib".into());
        }
        LinkOutputKind::SharedLibrary => args.push("-shared".into()),
    }
    append_paths(&mut args, &config.object_files);
    append_paths(&mut args, &config.runtime_libraries);
    args.push("-o".into());
    args.push(config.output_path.as_os_str().to_os_string());
    if target_family == LinkTargetFamily::Unix {
        args.push("-lm".into());
    }
    args.extend(config.extra_args.iter().cloned());
    args
}

fn unix_ld_args(target_family: LinkTargetFamily, config: &LinkerConfig) -> Vec<OsString> {
    let mut args = Vec::new();
    match config.output_kind {
        LinkOutputKind::Executable => {}
        LinkOutputKind::SharedLibrary if target_family == LinkTargetFamily::Darwin => {
            args.push("-dylib".into());
        }
        LinkOutputKind::SharedLibrary => args.push("-shared".into()),
    }
    append_paths(&mut args, &config.object_files);
    append_paths(&mut args, &config.runtime_libraries);
    args.push("-o".into());
    args.push(config.output_path.as_os_str().to_os_string());
    match target_family {
        LinkTargetFamily::Darwin => args.push("-lSystem".into()),
        LinkTargetFamily::Unix => {
            args.push("-lc".into());
            args.push("-lm".into());
        }
        LinkTargetFamily::WindowsMsvc => {}
    }
    args.extend(config.extra_args.iter().cloned());
    args
}

fn msvc_link_args(config: &LinkerConfig) -> Vec<OsString> {
    let mut args = vec!["/NOLOGO".into()];
    if config.output_kind == LinkOutputKind::SharedLibrary {
        args.push("/DLL".into());
    }
    args.push(format!("/OUT:{}", config.output_path.display()).into());
    append_paths(&mut args, &config.object_files);
    append_paths(&mut args, &config.runtime_libraries);
    args.push("msvcrt.lib".into());
    args.extend(config.extra_args.iter().cloned());
    args
}

fn append_paths(args: &mut Vec<OsString>, paths: &[PathBuf]) {
    args.extend(paths.iter().map(|path| path.as_os_str().to_os_string()));
}

fn is_clang_like(program: &Path) -> bool {
    program
        .file_name()
        .and_then(OsStr::to_str)
        .map(|name| name.contains("clang"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN_OBJ: &str = "main.o";
    const MAIN_OBJ_MSVC: &str = "main.obj";
    const RUNTIME_LIB: &str = "libsjulia_runtime.a";
    const LINUX_SHARED_LIB: &str = "libapp.so";
    const DARWIN_SHARED_LIB: &str = "libapp.dylib";
    const WINDOWS_SHARED_LIB: &str = "app.dll";

    fn strings(args: &[OsString]) -> Vec<String> {
        args.iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn linux_cc_driver_orders_objects_runtime_and_libm_issue_7089() {
        let mut config = LinkerConfig::new("app");
        config.object_files.push(MAIN_OBJ.into());
        config.runtime_libraries.push(RUNTIME_LIB.into());
        config.target_triple = Some("x86_64-unknown-linux-gnu".to_string());
        config.linker = Some("clang".into());

        let invocation = plan_system_link(&config).unwrap();

        assert_eq!(invocation.kind, LinkerKind::CcDriver);
        assert_eq!(invocation.target_family, LinkTargetFamily::Unix);
        assert_eq!(
            strings(&invocation.args),
            vec![
                "--target=x86_64-unknown-linux-gnu",
                MAIN_OBJ,
                RUNTIME_LIB,
                "-o",
                "app",
                "-lm"
            ]
        );
    }

    #[test]
    fn linux_lld_driver_links_libc_before_libm_issue_7089() {
        let mut config = LinkerConfig::new("app");
        config.object_files.push(MAIN_OBJ.into());
        config.target_triple = Some("x86_64-unknown-linux-gnu".to_string());
        config.linker = Some("ld.lld".into());

        let invocation = plan_system_link(&config).unwrap();

        assert_eq!(invocation.kind, LinkerKind::UnixLd);
        assert_eq!(
            strings(&invocation.args),
            vec![MAIN_OBJ, "-o", "app", "-lc", "-lm"]
        );
    }

    #[test]
    fn linux_cc_driver_shared_library_uses_shared_flag_issue_7085() {
        let mut config = LinkerConfig::new(LINUX_SHARED_LIB);
        config.object_files.push(MAIN_OBJ.into());
        config.target_triple = Some("x86_64-unknown-linux-gnu".to_string());
        config.linker = Some("clang".into());
        config.output_kind = LinkOutputKind::SharedLibrary;

        let invocation = plan_system_link(&config).unwrap();

        assert_eq!(
            strings(&invocation.args),
            vec![
                "--target=x86_64-unknown-linux-gnu",
                "-shared",
                MAIN_OBJ,
                "-o",
                LINUX_SHARED_LIB,
                "-lm"
            ]
        );
    }

    #[test]
    fn darwin_cc_driver_uses_libsystem_implicitly_issue_7089() {
        let mut config = LinkerConfig::new("app");
        config.object_files.push(MAIN_OBJ.into());
        config.target_triple = Some("x86_64-apple-darwin".to_string());
        config.linker = Some("cc".into());

        let invocation = plan_system_link(&config).unwrap();

        assert_eq!(invocation.kind, LinkerKind::CcDriver);
        assert_eq!(invocation.target_family, LinkTargetFamily::Darwin);
        assert_eq!(strings(&invocation.args), vec![MAIN_OBJ, "-o", "app"]);
    }

    #[test]
    fn darwin_cc_driver_shared_library_uses_dynamiclib_issue_7085() {
        let mut config = LinkerConfig::new(DARWIN_SHARED_LIB);
        config.object_files.push(MAIN_OBJ.into());
        config.target_triple = Some("x86_64-apple-darwin".to_string());
        config.linker = Some("clang".into());
        config.output_kind = LinkOutputKind::SharedLibrary;

        let invocation = plan_system_link(&config).unwrap();

        assert_eq!(
            strings(&invocation.args),
            vec![
                "--target=x86_64-apple-darwin",
                "-dynamiclib",
                MAIN_OBJ,
                "-o",
                DARWIN_SHARED_LIB
            ]
        );
    }

    #[test]
    fn windows_msvc_link_adds_out_and_crt_issue_7089() {
        let mut config = LinkerConfig::new("app.exe");
        config.object_files.push(MAIN_OBJ_MSVC.into());
        config.target_triple = Some("x86_64-pc-windows-msvc".to_string());
        config.linker = Some("lld-link".into());

        let invocation = plan_system_link(&config).unwrap();

        assert_eq!(invocation.kind, LinkerKind::MsvcLink);
        assert_eq!(invocation.target_family, LinkTargetFamily::WindowsMsvc);
        assert_eq!(
            strings(&invocation.args),
            vec!["/NOLOGO", "/OUT:app.exe", MAIN_OBJ_MSVC, "msvcrt.lib"]
        );
    }

    #[test]
    fn windows_msvc_link_shared_library_uses_dll_issue_7085() {
        let mut config = LinkerConfig::new(WINDOWS_SHARED_LIB);
        config.object_files.push(MAIN_OBJ_MSVC.into());
        config.target_triple = Some("x86_64-pc-windows-msvc".to_string());
        config.linker = Some("lld-link".into());
        config.output_kind = LinkOutputKind::SharedLibrary;

        let invocation = plan_system_link(&config).unwrap();

        assert_eq!(
            strings(&invocation.args),
            vec![
                "/NOLOGO",
                "/DLL",
                "/OUT:app.dll",
                MAIN_OBJ_MSVC,
                "msvcrt.lib"
            ]
        );
    }

    #[test]
    fn linker_plan_requires_objects_issue_7089() {
        let mut config = LinkerConfig::new("app");
        config.linker = Some("cc".into());

        assert_eq!(
            plan_system_link(&config).unwrap_err(),
            LinkerError::NoObjectFiles
        );
    }

    #[test]
    fn link_objects_reports_launch_failure_issue_7089() {
        let mut config = LinkerConfig::new("app");
        config.object_files.push(MAIN_OBJ.into());
        config.linker = Some("/definitely/missing/sjulia-linker".into());

        let err = link_objects(&config).unwrap_err();

        assert!(matches!(err, LinkerError::LinkerLaunchFailed { .. }));
        assert!(err.to_string().contains("failed to run"));
    }
}
