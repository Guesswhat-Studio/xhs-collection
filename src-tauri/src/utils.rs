use std::path::PathBuf;

pub(crate) fn display_path(path: PathBuf) -> String {
    path.to_string_lossy().to_string()
}
