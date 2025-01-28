use std::path::{Path, PathBuf};

/// a temporary trait to implement currently unstable methods
pub trait PathBufExt {
    /// https://doc.rust-lang.org/std/path/struct.PathBuf.html#method.leak
    fn leak<'a>(self) -> &'a mut Path;
}
impl PathBufExt for PathBuf {
    fn leak<'a>(self) -> &'a mut Path {
        Box::leak(self.into_boxed_path())
    }
}
