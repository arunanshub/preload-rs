use std::path::Path;

/// Check if a file path is accepted based on the exeprefixes.
///
/// ```
/// # use kernel::utils::accept_file;
/// let exeprefixes = [
///     "/usr/bin",
///     "/usr/sbin",
///     // ignore anything in personal dir
///     "!/home/user/personal",
///     // accept anything in `acceptedfolder` that is inside `personal` folder
///     "/home/user/personal/acceptedfolder"
/// ];
///
/// assert!(accept_file("/usr/bin/ls", &exeprefixes));
/// assert!(accept_file("/home/user/foobar", &exeprefixes));
/// assert!(!accept_file("/home/user/personal/notaccept", &exeprefixes));
/// assert!(accept_file("/home/user/personal/acceptedfolder/file", &exeprefixes));
/// // by default it accepts path that does not match any exeprefix
/// assert!(accept_file("/no/match", &exeprefixes));
/// ```
#[inline]
pub fn accept_file<T: AsRef<str>>(path: impl AsRef<Path>, exeprefixes: &[T]) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accept_file() {
        let exeprefixes = [
            "/usr/bin",
            "/usr/sbin",
            "!/home/user/personal",
            "/home/user/personal/acceptedfolder",
        ];

        assert!(accept_file("/usr/bin/ls", &exeprefixes));
        assert!(accept_file("/home/user/foobar", &exeprefixes));
        assert!(!accept_file("/home/user/personal/notaccept", &exeprefixes));
        assert!(accept_file(
            "/home/user/personal/acceptedfolder/file",
            &exeprefixes
        ));
        assert!(accept_file("/no/match", &exeprefixes));
    }

    #[test]
    fn test_sanitize_file() {
        let path = Path::new("/bin/bash.#prelink#.12345");
        assert_eq!(sanitize_file(&path), Some(Path::new("/bin/bash")));

        let path = Path::new("/bin/bash");
        assert_eq!(sanitize_file(&path), Some(Path::new("/bin/bash")));

        let path = Path::new("/bin/bash(deleted)");
        assert_eq!(sanitize_file(&path), None);
    }
}
