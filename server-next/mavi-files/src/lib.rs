//! File storage adapters for site-owned binary data.
//!
//! The media domain stores ownership and metadata in `PostgreSQL`, while this
//! crate stores the bytes behind the [`mavi_core::ports::FileStore`] port.
//! Every adapter receives a [`SiteContext`] and namespaces its key by site so
//! a storage key can never accidentally address another tenant's object.

use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
};

use mavi_core::{
    MaviError, Result, SiteContext, SiteId,
    ports::{BoxFuture, FileStore},
};

const INVALID_FILE_PATH: &str = "invalid_file_storage_path";
type MemoryFiles = BTreeMap<(SiteId, String), Vec<u8>>;

/// A durable local-directory implementation for self-host deployments.
#[derive(Clone, Debug)]
pub struct DirectoryFileStore {
    root: Arc<PathBuf>,
}

impl DirectoryFileStore {
    #[must_use]
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self {
            root: Arc::new(root.into()),
        }
    }

    fn path(&self, context: &SiteContext, key: &str) -> Result<PathBuf> {
        let relative = safe_relative_path(key)?;
        Ok(self.root.join(context.site_id.to_string()).join(relative))
    }
}

impl FileStore for DirectoryFileStore {
    fn put<'a>(
        &'a self,
        context: &'a SiteContext,
        key: &'a str,
        bytes: Vec<u8>,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let target = self.path(context, key)?;
            let parent = target.parent().ok_or(MaviError::Internal)?;
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|_| MaviError::Internal)?;

            let temporary = target.with_file_name(format!(
                "{}.part-{}",
                target
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or(MaviError::Internal)?,
                uuid::Uuid::now_v7()
            ));
            tokio::fs::write(&temporary, bytes)
                .await
                .map_err(|_| MaviError::Internal)?;
            if let Err(error) = tokio::fs::rename(&temporary, &target).await {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    // Unix rename replaces atomically; this fallback keeps
                    // the adapter usable on platforms where it does not.
                    if tokio::fs::remove_file(&target).await.is_ok()
                        && tokio::fs::rename(&temporary, &target).await.is_ok()
                    {
                        return Ok(());
                    }
                }
                let _ = tokio::fs::remove_file(&temporary).await;
                return Err(MaviError::Internal);
            }
            Ok(())
        })
    }

    fn get<'a>(&'a self, context: &'a SiteContext, key: &'a str) -> BoxFuture<'a, Result<Vec<u8>>> {
        Box::pin(async move {
            let path = self.path(context, key)?;
            match tokio::fs::read(path).await {
                Ok(bytes) => Ok(bytes),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    Err(MaviError::NotFound {
                        resource: "file_blob",
                    })
                }
                Err(_) => Err(MaviError::Internal),
            }
        })
    }

    fn remove<'a>(&'a self, context: &'a SiteContext, key: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let path = self.path(context, key)?;
            match tokio::fs::remove_file(path).await {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(_) => Err(MaviError::Internal),
            }
        })
    }
}

/// An in-memory adapter for domain and HTTP tests.
#[derive(Clone, Debug, Default)]
pub struct InMemoryFileStore {
    files: Arc<Mutex<MemoryFiles>>,
}

impl InMemoryFileStore {
    #[must_use]
    pub fn contains(&self, context: &SiteContext, key: &str) -> bool {
        let Ok(key) = safe_relative_path(key) else {
            return false;
        };
        self.files.lock().is_ok_and(|files| {
            files.contains_key(&(context.site_id, key.to_string_lossy().into_owned()))
        })
    }
}

impl FileStore for InMemoryFileStore {
    fn put<'a>(
        &'a self,
        context: &'a SiteContext,
        key: &'a str,
        bytes: Vec<u8>,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let key = safe_relative_path(key)?.to_string_lossy().into_owned();
            self.files
                .lock()
                .map_err(|_| MaviError::Internal)?
                .insert((context.site_id, key), bytes);
            Ok(())
        })
    }

    fn get<'a>(&'a self, context: &'a SiteContext, key: &'a str) -> BoxFuture<'a, Result<Vec<u8>>> {
        Box::pin(async move {
            let key = safe_relative_path(key)?.to_string_lossy().into_owned();
            self.files
                .lock()
                .map_err(|_| MaviError::Internal)?
                .get(&(context.site_id, key))
                .cloned()
                .ok_or(MaviError::NotFound {
                    resource: "file_blob",
                })
        })
    }

    fn remove<'a>(&'a self, context: &'a SiteContext, key: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let key = safe_relative_path(key)?.to_string_lossy().into_owned();
            self.files
                .lock()
                .map_err(|_| MaviError::Internal)?
                .remove(&(context.site_id, key));
            Ok(())
        })
    }
}

fn safe_relative_path(key: &str) -> Result<PathBuf> {
    if key.is_empty() || key.contains('\\') {
        return Err(MaviError::validation(INVALID_FILE_PATH));
    }

    let mut safe = PathBuf::new();
    for component in Path::new(key).components() {
        match component {
            Component::Normal(part) => safe.push(part),
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => return Err(MaviError::validation(INVALID_FILE_PATH)),
        }
    }

    if safe.as_os_str().is_empty() {
        Err(MaviError::validation(INVALID_FILE_PATH))
    } else {
        Ok(safe)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_paths_that_can_escape_the_adapter_root() {
        for key in [
            "",
            ".",
            "..",
            "../secret",
            "a/../../secret",
            "/etc/passwd",
            "a\\b",
        ] {
            assert!(safe_relative_path(key).is_err(), "accepted {key:?}");
        }
        assert!(safe_relative_path("ab/a..b.png").is_ok());
    }

    #[tokio::test]
    async fn local_store_writes_atomically_and_removes_idempotently() {
        let root = std::env::temp_dir().join(format!("mavi-files-{}", uuid::Uuid::now_v7()));
        let store = DirectoryFileStore::at(&root);
        let context = SiteContext::public(mavi_core::SiteId::new());

        store
            .put(&context, "ab/file.png", b"bytes".to_vec())
            .await
            .expect("put");
        store
            .put(&context, "ab/file.png", b"replacement".to_vec())
            .await
            .expect("replace");
        assert_eq!(
            store.get(&context, "ab/file.png").await.expect("get"),
            b"replacement"
        );
        store.remove(&context, "ab/file.png").await.expect("remove");
        store
            .remove(&context, "ab/file.png")
            .await
            .expect("idempotent remove");
        assert!(store.get(&context, "ab/file.png").await.is_err());

        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn memory_store_isolated_by_site() {
        let store = InMemoryFileStore::default();
        let first = SiteContext::public(mavi_core::SiteId::new());
        let second = SiteContext::public(mavi_core::SiteId::new());

        store.put(&first, "a/file.bin", vec![1]).await.expect("put");
        assert!(store.get(&first, "a/file.bin").await.is_ok());
        assert!(store.get(&second, "a/file.bin").await.is_err());
    }
}
