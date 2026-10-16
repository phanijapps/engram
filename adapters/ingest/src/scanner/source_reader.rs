//! Root-confined, bounded file admission; parser orchestration consumes bytes.
use std::{
    io::{self, Read},
    path::Path,
};

pub(super) struct SourceReader {
    root: cap_std::fs::Dir,
}
impl SourceReader {
    pub(super) fn open(root: &Path) -> io::Result<Self> {
        Ok(Self {
            root: cap_std::fs::Dir::open_ambient_dir(root, cap_std::ambient_authority())?,
        })
    }
    pub(super) fn read(&self, path: &str, max_bytes: u64) -> io::Result<Vec<u8>> {
        let file = self.root.open(path)?;
        if !file.metadata()?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "source is not a regular file",
            ));
        }
        let mut bytes = Vec::new();
        file.take(max_bytes.saturating_add(1))
            .read_to_end(&mut bytes)?;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_bytes_and_growth_bound() {
        let root = tempfile::tempdir().unwrap();
        let reader = SourceReader::open(root.path()).unwrap();
        let bytes = "\u{feff}const π=1;\r\n".as_bytes();
        std::fs::write(root.path().join("a.ts"), bytes).unwrap();
        assert_eq!(reader.read("a.ts", 100).unwrap(), bytes);
        // The inode was accepted at walk time, then grew before open/read.
        std::fs::write(root.path().join("a.ts"), [b'x'; 4096]).unwrap();
        assert_eq!(reader.read("a.ts", 100).unwrap().len(), 101);
    }
    #[cfg(unix)]
    #[test]
    fn final_and_ancestor_symlink_swaps_cannot_read_outside_root() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("a.ts"), "outside-secret").unwrap();
        std::fs::create_dir(root.path().join("src")).unwrap();
        std::fs::write(root.path().join("src/a.ts"), "accepted").unwrap();
        let reader = SourceReader::open(root.path()).unwrap();
        let accepted = std::fs::canonicalize(root.path().join("src/a.ts")).unwrap();
        assert!(accepted.starts_with(root.path()));
        std::fs::remove_file(&accepted).unwrap();
        symlink(outside.path().join("a.ts"), &accepted).unwrap();
        assert!(reader.read("src/a.ts", 100).is_err());
        std::fs::remove_file(&accepted).unwrap();
        std::fs::remove_dir(root.path().join("src")).unwrap();
        symlink(outside.path(), root.path().join("src")).unwrap();
        assert!(reader.read("src/a.ts", 100).is_err());
    }
}
