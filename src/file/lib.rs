
pub fn delete_file(path: &str) {
    let _ = std::fs::remove_file(path);
}