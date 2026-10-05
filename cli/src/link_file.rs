use crate::error::CliError;
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::Path,
};

pub(crate) struct LinkFile {
    file: File,
    complete: bool,
    path: std::path::PathBuf,
}
impl LinkFile {
    fn verify_identity(&self) -> Result<(), CliError> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            let actual = self.file.metadata().map_err(|_| CliError::LinkFile)?;
            let named = std::fs::symlink_metadata(&self.path).map_err(|_| CliError::LinkFile)?;
            if !named.is_file()
                || actual.dev() != named.dev()
                || actual.ino() != named.ino()
                || actual.nlink() != 1
                || actual.permissions().mode() & 0o777 != 0o600
            {
                return Err(CliError::LinkFile);
            }
            Ok(())
        }
        #[cfg(not(unix))]
        {
            Err(CliError::LinkFile)
        }
    }
    pub(crate) fn persist(&mut self, url: &str) -> Result<(), CliError> {
        self.verify_identity()?;
        self.file
            .write_all(url.as_bytes())
            .and_then(|()| self.file.write_all(b"\n"))
            .and_then(|()| self.file.sync_all())
            .map_err(|_| CliError::LinkFile)?;
        self.verify_identity()?;
        self.complete = true;
        Ok(())
    }
    pub(crate) fn reserve(path: &Path) -> Result<Self, CliError> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            let file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path)
                .map_err(|_| CliError::LinkFile)?;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))
                .map_err(|_| CliError::LinkFile)?;
            if file
                .metadata()
                .map_err(|_| CliError::LinkFile)?
                .permissions()
                .mode()
                & 0o777
                != 0o600
            {
                return Err(CliError::LinkFile);
            }
            Ok(Self {
                file,
                complete: false,
                path: path.to_owned(),
            })
        }
        #[cfg(not(unix))]
        {
            let _ = path;
            // No unverified emulation of Unix-private permissions.
            Err(CliError::LinkFile)
        }
    }
}

impl Drop for LinkFile {
    fn drop(&mut self) {
        if !self.complete {
            // There is no portable atomic unlink-if-inode-matches. Leave an empty
            // private reservation instead of racing a replacement pathname.
            let _ = self.file.set_len(0);
            let _ = self.file.sync_all();
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "Test fixtures fail fast without logging secrets."
)]
mod tests {
    use super::*;
    #[test]
    fn existing_symlinks_and_unwritable_paths_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("symlink");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path().join("missing"), &path).unwrap();
            assert!(LinkFile::reserve(&path).is_err());
        }
        assert!(LinkFile::reserve(&dir.path().join("missing-parent").join("link")).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn weakened_permissions_or_hardlinks_fail_before_secret_write() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        for hardlink in [true, false] {
            let path = dir.path().join(if hardlink { "hardlink" } else { "mode" });
            let mut file = LinkFile::reserve(&path).unwrap();
            if hardlink {
                std::fs::hard_link(&path, dir.path().join("alias")).unwrap();
            } else {
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
            }
            let url = crate::fixtures::enrollment_url();
            assert!(file.persist(&url).is_err());
            drop(file);
            assert!(std::fs::read(&path).unwrap().is_empty());
        }
    }
    #[test]
    fn write_failure_is_typed_without_secret_sources() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("link");
        let mut file = LinkFile::reserve(&path).unwrap();
        file.file = std::fs::File::open(&path).unwrap();
        let url = crate::fixtures::enrollment_url();
        let error = file.persist(&url).unwrap_err();
        assert_eq!(error, CliError::LinkFile);
        assert!(std::error::Error::source(&error).is_none());
        assert!(!format!("{error} {error:?}").contains(&*url));
        drop(file);
        assert!(std::fs::read(&path).unwrap().is_empty());
    }

    #[test]
    fn replaced_path_is_rejected_before_secret_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("link");
        let original = dir.path().join("original");
        let mut file = LinkFile::reserve(&path).unwrap();
        std::fs::rename(&path, &original).unwrap();
        std::fs::write(&path, b"replacement").unwrap();
        assert!(file.persist(&crate::fixtures::enrollment_url()).is_err());
        drop(file);
        assert!(std::fs::read(&original).unwrap().is_empty());
        assert!(std::fs::read(&path).unwrap() == b"replacement");
    }

    #[test]
    fn failure_cleanup_truncates_only_owned_handle_not_replaced_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("link");
        let owned_path = dir.path().join("moved-owned-link");
        let mut file = LinkFile::reserve(&path).unwrap();
        file.file.write_all(b"partial").unwrap();
        std::fs::rename(&path, &owned_path).unwrap();
        std::fs::write(&path, b"replacement").unwrap();
        drop(file);
        assert!(std::fs::read(&owned_path).unwrap().is_empty());
        assert!(std::fs::read(&path).unwrap() == b"replacement");
    }

    #[test]
    fn persists_link_without_displaying_or_reopening_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("link");
        let mut file = LinkFile::reserve(&path).unwrap();
        let url = crate::fixtures::enrollment_url();
        file.persist(&url).unwrap();
        drop(file);
        assert!(std::fs::read_to_string(&path).unwrap() == format!("{}\n", *url));
    }

    #[test]
    fn reserves_new_private_file_before_network() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("link");
        let _file = LinkFile::reserve(&path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert!(LinkFile::reserve(&path).is_err());
    }
}
