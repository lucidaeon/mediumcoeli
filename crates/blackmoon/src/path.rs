//! Display helpers for local filesystem paths.

use std::path::Path;

/// Render a filesystem path for display, collapsing repeated separators and
/// normalizing `.` components. Lexical only -- never touches the filesystem.
#[must_use]
pub(crate) fn display_path(p: &Path) -> String {
    p.components()
        .collect::<std::path::PathBuf>()
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::display_path;
    use std::path::Path;

    #[test]
    fn collapses_repeated_separators_absolute() {
        assert_eq!(display_path(Path::new("/a//b/")), "/a/b");
    }

    #[test]
    fn collapses_repeated_separators_relative() {
        assert_eq!(display_path(Path::new("a//b")), "a/b");
    }
}
