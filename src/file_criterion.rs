use std::{ffi::OsStr, fmt::Display, path::Path};

/// A file criterion defines a set of matching files to be parsed
/// using a specific grammar.
#[derive(Debug, Clone, PartialEq)]
pub enum FileCriterion {
    /// Files names ending with a particular extension
    ByExt(&'static str),
    /// Only this particular file nameç
    ByName(&'static str),
}

impl FileCriterion {
    /// Checks if the file at the given path matches the criterion
    pub(crate) fn matches<P: AsRef<Path>>(&self, path: P) -> bool {
        let path = path.as_ref();
        match self {
            Self::ByExt(target_ext) => {
                path.extension().is_some_and(|extension|
                    // NOTE: the comparison should be case-insensitive, see
                    // https://rust-lang.github.io/rust-clippy/master/index.html#case_sensitive_file_extension_comparisons
                    extension.eq_ignore_ascii_case(OsStr::new(target_ext)))
            }
            Self::ByName(target_name) => path
                .file_name()
                .is_some_and(|name| name == OsStr::new(target_name)),
        }
    }

    /// The criterion treated sa language identifier (for instance to identify
    /// the language via the --language CLI option)
    pub(crate) fn as_alternate_name(&self) -> &'static str {
        match self {
            Self::ByExt(extension) => extension,
            Self::ByName(full_name) => full_name,
        }
    }
}

/// We display the criteria using the format that Git understands in
/// its gitattributes files.
impl Display for FileCriterion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ByExt(extension) => write!(f, "*.{extension}"),
            Self::ByName(name) => write!(f, "{name}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn by_ext() {
        let criterion = FileCriterion::ByExt("rs");
        assert!(criterion.matches("/home/user/lib.rs"));
        assert!(!criterion.matches("path/to/rs"));
        assert_eq!(format!("{criterion}"), "*.rs");
        assert_eq!(criterion.as_alternate_name(), "rs");
    }

    #[test]
    fn by_name() {
        let criterion = FileCriterion::ByName("Makefile");
        assert!(criterion.matches("/tmp/Makefile"));
        assert!(!criterion.matches("Makefile.in"));
        assert_eq!(format!("{criterion}"), "Makefile");
        assert_eq!(criterion.as_alternate_name(), "Makefile");
    }
}
