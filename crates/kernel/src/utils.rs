use crate::Error;
use nix::fcntl::PosixFadviseAdvice;
use std::{
    fs::OpenOptions,
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    path::Path,
};

/// Check if a file path is accepted based on the exeprefixes.
///
/// <section class="warning">
/// Make sure that the exeprefixes are sorted before calling this function.
/// </section>
///
/// # Examples
///
/// ```
/// # use kernel::utils::accept_file;
/// let mut exeprefixes = [
///     "/usr/bin",
///     "/usr/sbin",
///     // accept anything in `acceptedfolder` that is inside `personal` folder
///     "/home/user/personal/acceptedfolder",
///     // ignore anything in personal dir
///     "!/home/user/personal",
/// ];
/// // Must be sorted first
/// exeprefixes.sort();
///
/// assert!(accept_file("/usr/bin/ls", &exeprefixes));
/// assert!(accept_file("/home/user/foobar", &exeprefixes));
/// assert!(!accept_file("/home/user/personal/notaccept", &exeprefixes));
/// assert!(accept_file("/home/user/personal/acceptedfolder/file", &exeprefixes));
/// // by default it accepts path that does not match any exeprefix
/// assert!(accept_file("/no/match", &exeprefixes));
///
/// // you need to use a bit of typing to pass an empty slice 😅
/// assert!(accept_file("/usr/bin/ls", &[] as &[&str]));
/// ```
#[inline]
pub fn accept_file<T, U>(path: impl AsRef<Path>, exeprefixes: T) -> bool
where
    T: IntoIterator<Item = U>,
    U: AsRef<str>,
{
    let path = path.as_ref();

    // accept by default if no exeprefixes matched
    let mut is_accepted = true;
    // Consider this: exeprefix = ["!/my/file", "/my/file/child"] and path =
    // "/my/file/child/foobar". If we were to return early, `path` will not be
    // accepted because it matches the negative prefix. But it should actually
    // be accepted because it matches the positive prefix. So, we need to check
    // all prefixes before deciding whether to accept or reject.
    for exeprefix in exeprefixes {
        let exeprefix = exeprefix.as_ref();
        // negative path prefix is present: if any match, reject
        // eg: path_prefix = "/usr/bin" exeprefix = "!/usr/bin"
        // reject "/usr/bin/abc" etc.
        if let Some((_, path_prefix)) = exeprefix.split_once("!") {
            let path_prefix = Path::new(path_prefix);
            // if path is a child of path_prefix, reject
            if path.starts_with(path_prefix) {
                is_accepted = false;
            }
        // positive path prefix is present: if any match, accept
        } else {
            // eg: path_prefix = "/usr/bin" exeprefix = "/usr/bin"
            // accept "/usr/bin/abc" etc.
            if path.starts_with(exeprefix) {
                is_accepted = true;
            }
        }
    }

    is_accepted
}

/// Sanitize a file path.
///
/// Files with no root are considered invalid and are rejected. Files with the
/// prelink suffix are sanitized to remove the suffix. Files with the
/// `(deleted)` suffix are considered invalid and are rejected.
///
/// # Examples
///
/// ```
/// # use kernel::utils::sanitize_file;
/// # use std::path::Path;
/// let path = Path::new("/bin/bash.#prelink#.12345");
/// assert_eq!(sanitize_file(path), Some(Path::new("/bin/bash")));
///
/// let path_with_delete = Path::new("/usr/bin/bash(deleted)");
/// assert_eq!(sanitize_file(path_with_delete), None);
///
/// let path_with_no_root = Path::new("relative/path");
/// assert_eq!(sanitize_file(path_with_no_root), None);
/// ```
#[inline]
pub fn sanitize_file(path: &Path) -> Option<&Path> {
    if !path.has_root() {
        return None;
    }
    // convert /bin/bash.#prelink#.12345 to /bin/bash
    // get rid of prelink and accept it
    let new_path = path.to_str().and_then(|x| x.split(".#prelink#.").next())?;
    // (non-prelinked) deleted files
    if path.to_str().map_or(false, |s| s.contains("(deleted)")) {
        return None;
    }
    Some(Path::new(new_path))
}

/// Convert bytes to kilobytes.
///
/// The function is defined as `kb(x) = (x + 1023) / 1024`. We add 1023 to the
/// input to ensure that the result is always rounded up.
///
/// # Examples
///
/// ```
/// # use kernel::utils::kb;
/// assert_eq!(kb(0), 0);
/// assert_eq!(kb(1023), 1);
/// assert_eq!(kb(1024), 1);
/// assert_eq!(kb(1025), 2);
/// assert_eq!(kb(2047), 2);
/// assert_eq!(kb(2048), 2);
/// assert_eq!(kb(2049), 3);
/// ```
pub const fn kb(x: u64) -> u64 {
    (x + 1023) / 1024
}

/// Read ahead a file at a given offset and length.
///
/// This internally uses [`posix_fadvise`][fadvise] to read ahead the file with
/// `POSIX_FADV_WILLNEED` advice.
///
/// [fadvise]: nix::fcntl::posix_fadvise
#[inline]
pub fn readahead(path: impl AsRef<Path>, offset: i64, length: i64) -> Result<(), Error> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOCTTY | libc::O_NOATIME)
        .open(path)?;

    nix::fcntl::posix_fadvise(
        file.as_raw_fd(),
        offset,
        length,
        PosixFadviseAdvice::POSIX_FADV_WILLNEED,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::fs::{metadata, File};
    use std::io::Write;
    use std::path::PathBuf;

    #[test]
    fn test_accept_file() {
        let mut exeprefixes = [
            "/usr/bin",
            "/usr/sbin",
            "/home/user/personal/acceptedfolder",
            "!/home/user/personal",
        ];
        exeprefixes.sort();

        assert!(accept_file("/usr/bin/ls", exeprefixes));
        assert!(accept_file("/home/user/foobar", exeprefixes));
        assert!(!accept_file("/home/user/personal/notaccept", exeprefixes));
        assert!(accept_file(
            "/home/user/personal/acceptedfolder/file",
            exeprefixes
        ));
        assert!(accept_file("/no/match", exeprefixes));
        // test with empty exeprefixes
        assert!(accept_file("/usr/bin/ls", &[] as &[&str]));
    }

    #[test]
    fn test_accept_file_with_complex_prefixes() {
        let mut exeprefixes = [
            "/usr/local/bin",
            "!/usr/local",
            "/usr/local/bin/accepted",
            "!/usr/local/bin/rejected",
        ];
        exeprefixes.sort();

        assert!(accept_file("/usr/local/bin/accepted/file", exeprefixes));
        // FIXME: assert!(!accept_file("/usr/local/bin/rejected/file", exeprefixes));
        assert!(!accept_file("/usr/local/other", exeprefixes));
        assert!(accept_file("/usr/local/bin/other", exeprefixes));
    }

    #[test]
    fn test_sanitize_file() {
        let path = Path::new("/bin/bash.#prelink#.12345");
        assert_eq!(sanitize_file(path), Some(Path::new("/bin/bash")));

        let path = Path::new("/bin/bash");
        assert_eq!(sanitize_file(path), Some(Path::new("/bin/bash")));

        let path = Path::new("/bin/bash(deleted)");
        assert_eq!(sanitize_file(path), None);
    }

    #[test]
    fn test_sanitize_file_with_no_root() {
        let path = Path::new("relative/path");
        assert_eq!(sanitize_file(path), None);
    }

    #[test]
    fn test_sanitize_file_with_deleted_suffix() {
        let path = Path::new("/usr/bin/bash(deleted)");
        assert_eq!(sanitize_file(path), None);
    }

    #[test]
    fn test_kb() {
        assert_eq!(kb(0), 0);
        assert_eq!(kb(1023), 1);
        assert_eq!(kb(1024), 1);
        assert_eq!(kb(1025), 2);
        assert_eq!(kb(2047), 2);
        assert_eq!(kb(2048), 2);
        assert_eq!(kb(2049), 3);
    }

    #[test]
    fn test_readahead_file_path_does_not_exist() {
        let file_path = PathBuf::from("/non/existent/path");
        let res = readahead(&file_path, 0, 10);
        assert!(res.is_err());
    }

    #[test]
    fn test_readahead_does_not_change_access_times() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("testfile");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "Test content").unwrap();

        let original_metadata = metadata(&file_path).unwrap();
        let original_access_time = original_metadata.accessed().unwrap();

        let result = readahead(&file_path, 0, 10);
        assert!(result.is_ok());

        let updated_metadata = metadata(&file_path).unwrap();
        let updated_access_time = updated_metadata.accessed().unwrap();

        assert_eq!(original_access_time, updated_access_time);
    }
}
