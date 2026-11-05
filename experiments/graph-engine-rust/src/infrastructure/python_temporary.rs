use super::{filesystem, os_error, policy_files::NativePolicyFiles, temporary_files};
use crate::{
    application::{error::EngineResult, policy::PolicyFilePort},
    compat::repr::string_repr,
};
use std::{
    ffi::OsString,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

#[cfg(windows)]
pub const TMP_MAX: usize = 2_147_483_647;
#[cfg(unix)]
pub const TMP_MAX: usize = libc::TMP_MAX as usize;

pub fn random_name() -> io::Result<String> {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789_";
    let mut result = String::with_capacity(8);
    while result.len() != 8 {
        let mut bytes = [0; 32];
        getrandom::getrandom(&mut bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::Other, error.to_string()))?;
        for byte in bytes {
            if byte < 222 {
                result.push(char::from(ALPHABET[usize::from(byte) % ALPHABET.len()]));
                if result.len() == 8 {
                    break;
                }
            }
        }
    }
    Ok(result)
}

pub fn candidates() -> Vec<PathBuf> {
    let mut result = Vec::new();
    for name in ["TMPDIR", "TEMP", "TMP"] {
        if let Some(value) = std::env::var_os(name).filter(|value| !value.is_empty()) {
            result.push(value.into());
        }
    }
    #[cfg(windows)]
    {
        let home = std::env::var_os("USERPROFILE").or_else(|| {
            std::env::var_os("HOMEPATH").map(|path| {
                PathBuf::from(std::env::var_os("HOMEDRIVE").unwrap_or_default())
                    .join(path)
                    .into_os_string()
            })
        });
        let mut user = home.unwrap_or_else(|| OsString::from("~"));
        user.push("\\AppData\\Local\\Temp");
        result.push(user.into());
        let mut system = std::env::var_os("SYSTEMROOT").unwrap_or_else(|| "%SYSTEMROOT%".into());
        system.push("\\Temp");
        result.push(system.into());
        result.extend([r"c:\temp", r"c:\tmp", r"\temp", r"\tmp"].map(PathBuf::from));
    }
    #[cfg(unix)]
    {
        result.extend(["/tmp", "/var/tmp", "/usr/tmp"].map(PathBuf::from));
    }
    result.push(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    result
}

fn candidate_display(paths: &[PathBuf]) -> String {
    format!(
        "[{}]",
        paths
            .iter()
            .map(|path| string_repr(&filesystem::os_string(path.as_os_str()))
                .utf8()
                .unwrap())
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn probe(path: &Path) -> io::Result<()> {
    let mut descriptor = temporary_files::create_new(path)?;
    let written = descriptor.write(b"blat").map(|_| ());
    let closed = temporary_files::close(descriptor);
    let removed = std::fs::remove_file(path)
        .map_err(|error| os_error::with_path(error, os_error::OsApi::Native, Some(path), None));
    removed.and(closed).and(written)
}

pub fn find_usable(
    paths: &[PathBuf],
    names: &mut dyn FnMut() -> io::Result<String>,
) -> EngineResult<PathBuf> {
    let files = NativePolicyFiles::default();
    for original in paths {
        let directory = if original.as_os_str() == "." {
            original.clone()
        } else {
            files.absolute(original)?
        };
        for _ in 0..100 {
            let filename = directory.join(names()?);
            match probe(&filename) {
                Ok(()) => return Ok(directory),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error)
                    if error.kind() == io::ErrorKind::PermissionDenied
                        && cfg!(windows)
                        && temporary_files::writable_directory(&directory) =>
                {
                    continue
                }
                Err(_) => break,
            }
        }
    }
    Err(os_error::errno_message(
        2,
        io::ErrorKind::NotFound,
        &format!(
            "No usable temporary directory found in {}",
            candidate_display(paths)
        ),
    )
    .into())
}

#[derive(Default)]
pub struct TemporaryDirectoryCache {
    directory: Mutex<Option<PathBuf>>,
}
impl TemporaryDirectoryCache {
    pub fn get_or_try_init(
        &self,
        initialize: impl FnOnce() -> EngineResult<PathBuf>,
    ) -> EngineResult<PathBuf> {
        let mut cached = self
            .directory
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(directory) = cached.as_ref() {
            return Ok(directory.clone());
        }
        let directory = initialize()?;
        *cached = Some(directory.clone());
        Ok(directory)
    }
}
static DIRECTORY: TemporaryDirectoryCache = TemporaryDirectoryCache {
    directory: Mutex::new(None),
};
pub fn temporary_root() -> EngineResult<PathBuf> {
    DIRECTORY.get_or_try_init(|| find_usable(&candidates(), &mut random_name))
}

pub(crate) fn create_file(
    directory: &Path,
    prefix: &str,
    suffix: &str,
) -> EngineResult<(temporary_files::File, PathBuf)> {
    create_file_with(directory, prefix, suffix, TMP_MAX, &mut random_name)
}
pub(crate) fn create_file_with(
    directory: &Path,
    prefix: &str,
    suffix: &str,
    attempts: usize,
    names: &mut dyn FnMut() -> io::Result<String>,
) -> EngineResult<(temporary_files::File, PathBuf)> {
    let directory = NativePolicyFiles::default().absolute(directory)?;
    for _ in 0..attempts {
        let path = directory.join(format!("{prefix}{}{suffix}", names()?));
        match temporary_files::create_new(&path) {
            Ok(file) => return Ok((file, path)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error)
                if error.kind() == io::ErrorKind::PermissionDenied
                    && cfg!(windows)
                    && temporary_files::writable_directory(&directory) =>
            {
                continue
            }
            Err(error) => return Err(error.into()),
        }
    }
    Err(os_error::errno_message(
        17,
        io::ErrorKind::AlreadyExists,
        "No usable temporary file name found",
    )
    .into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::error::EngineError;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    struct TestDirectory(PathBuf);
    impl TestDirectory {
        fn new() -> Self {
            let parent = PathBuf::from(
                std::env::var_os("GRAPH_RUST_RUN_ARTIFACT_ROOT").expect("ART required"),
            )
            .join("fixtures");
            std::fs::create_dir_all(&parent).unwrap();
            let path = parent.join(format!(
                "python-temporary-{}-{}",
                std::process::id(),
                random_name().unwrap()
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            if std::thread::panicking() {
                return;
            }
            let parent = PathBuf::from(std::env::var_os("GRAPH_RUST_RUN_ARTIFACT_ROOT").unwrap())
                .join("fixtures")
                .canonicalize()
                .unwrap();
            let target = self.0.canonicalize().unwrap();
            assert_eq!(target.parent(), Some(parent.as_path()));
            assert!(target
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("python-temporary-"));
            std::fs::remove_dir_all(target).unwrap();
        }
    }
    #[test]
    fn explicit_art_candidates_handle_missing_file_and_directory_collisions() {
        let root = TestDirectory::new();
        let invalid = root.0.join("not-directory");
        std::fs::write(&invalid, b"sentinel").unwrap();
        let directory = root.0.join("usable");
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(directory.join("collide0"), b"unchanged").unwrap();
        std::fs::create_dir(directory.join("occupy00")).unwrap();
        let mut sequence = ["missing0", "missing1", "collide0", "occupy00", "usable00"].into_iter();
        let selected = find_usable(
            &[root.0.join("absent"), invalid.clone(), directory.clone()],
            &mut || Ok(sequence.next().unwrap().into()),
        )
        .unwrap();
        assert_eq!(
            selected,
            NativePolicyFiles::default().absolute(&directory).unwrap()
        );
        assert_eq!(
            std::fs::read(directory.join("collide0")).unwrap(),
            b"unchanged"
        );
        assert_eq!(std::fs::read(invalid).unwrap(), b"sentinel");
        assert!(!directory.join("usable00").exists());
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 2);
    }
    #[test]
    fn candidate_collision_limit_and_exhausted_names_preserve_errno_metadata() {
        let root = TestDirectory::new();
        let first = root.0.join("first");
        let second = root.0.join("second");
        std::fs::create_dir(&first).unwrap();
        std::fs::create_dir(&second).unwrap();
        std::fs::write(first.join("collide0"), b"sentinel").unwrap();
        let mut calls = 0;
        let selected = find_usable(&[first, second.clone()], &mut || {
            calls += 1;
            Ok("collide0".into())
        })
        .unwrap();
        assert_eq!(calls, 101);
        assert_eq!(
            selected,
            NativePolicyFiles::default().absolute(&second).unwrap()
        );
        std::fs::write(second.join(".register.collide0.tmp"), b"sentinel").unwrap();
        let mut calls = 0;
        let error = create_file_with(&second, ".register.", ".tmp", 3, &mut || {
            calls += 1;
            Ok("collide0".into())
        })
        .err()
        .unwrap();
        assert_eq!(calls, 3);
        let EngineError::Io(error) = error else {
            panic!("expected io failure");
        };
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(
            error.to_string(),
            "[Errno 17] No usable temporary file name found"
        );
        let metadata = error
            .get_ref()
            .unwrap()
            .downcast_ref::<os_error::OsFailure>()
            .unwrap();
        assert_eq!(metadata.errno, Some(17));
        assert_eq!(metadata.winerror, None);
        assert_eq!(metadata.filename, None);
        #[cfg(windows)]
        assert_eq!(TMP_MAX, 2_147_483_647);
    }
    #[test]
    fn failed_cache_initialization_retries_and_success_is_shared_between_threads() {
        let cache = Arc::new(TemporaryDirectoryCache::default());
        assert!(cache
            .get_or_try_init(|| Err(EngineError::state("FAILED_PROBE")))
            .is_err());
        let root = TestDirectory::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let threads = (0..8)
            .map(|_| {
                let cache = cache.clone();
                let calls = calls.clone();
                let path = root.0.clone();
                std::thread::spawn(move || {
                    cache
                        .get_or_try_init(|| {
                            calls.fetch_add(1, Ordering::SeqCst);
                            Ok(path)
                        })
                        .unwrap()
                })
            })
            .collect::<Vec<_>>();
        for thread in threads {
            assert_eq!(thread.join().unwrap(), root.0);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        for _ in 0..128 {
            let name = random_name().unwrap();
            assert_eq!(name.len(), 8);
            assert!(name
                .bytes()
                .all(|byte| b"abcdefghijklmnopqrstuvwxyz0123456789_".contains(&byte)));
        }
    }
}
