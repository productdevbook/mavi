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

    fn list<'a>(&'a self, context: &'a SiteContext) -> BoxFuture<'a, Result<Vec<String>>> {
        Box::pin(async move {
            let site_root = self.root.join(context.site_id.to_string());
            let mut directories = vec![site_root.clone()];
            let mut keys = Vec::new();

            while let Some(directory) = directories.pop() {
                let mut entries = match tokio::fs::read_dir(&directory).await {
                    Ok(entries) => entries,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(_) => return Err(MaviError::Internal),
                };

                while let Some(entry) = entries
                    .next_entry()
                    .await
                    .map_err(|_| MaviError::Internal)?
                {
                    let file_type = entry.file_type().await.map_err(|_| MaviError::Internal)?;
                    if file_type.is_dir() {
                        directories.push(entry.path());
                    } else if file_type.is_file() {
                        let entry_path = entry.path();
                        let relative = entry_path
                            .strip_prefix(&site_root)
                            .map_err(|_| MaviError::Internal)?;
                        if let Some(key) = normalized_key(relative) {
                            keys.push(key);
                        }
                    }
                }
            }

            keys.sort();
            Ok(keys)
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

    fn list<'a>(&'a self, context: &'a SiteContext) -> BoxFuture<'a, Result<Vec<String>>> {
        Box::pin(async move {
            let mut keys = self
                .files
                .lock()
                .map_err(|_| MaviError::Internal)?
                .iter()
                .filter(|((site_id, _), _)| *site_id == context.site_id)
                .map(|((_, key), _)| key.clone())
                .collect::<Vec<_>>();
            keys.sort();
            Ok(keys)
        })
    }
}

fn normalized_key(path: &Path) -> Option<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        let Component::Normal(part) = component else {
            return None;
        };
        parts.push(part.to_str()?.to_owned());
    }
    if parts.is_empty() {
        return None;
    }
    let key = parts.join("/");
    safe_relative_path(&key).ok()?;
    Some(key)
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
    async fn local_store_lists_only_regular_nested_files_for_one_site() {
        let root = std::env::temp_dir().join(format!("mavi-files-list-{}", uuid::Uuid::now_v7()));
        let store = DirectoryFileStore::at(&root);
        let first = SiteContext::public(mavi_core::SiteId::new());
        let second = SiteContext::public(mavi_core::SiteId::new());

        store
            .put(&first, "ab/nested/file.png", vec![1])
            .await
            .expect("put");
        store.put(&first, "root.bin", vec![2]).await.expect("put");
        store
            .put(&second, "ab/other.png", vec![3])
            .await
            .expect("put");

        assert_eq!(
            store.list(&first).await.expect("list"),
            vec!["ab/nested/file.png", "root.bin"]
        );
        assert_eq!(
            store.list(&second).await.expect("list"),
            vec!["ab/other.png"]
        );

        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn memory_store_isolated_by_site() {
        let store = InMemoryFileStore::default();
        let first = SiteContext::public(mavi_core::SiteId::new());
        let second = SiteContext::public(mavi_core::SiteId::new());

        store.put(&first, "a/file.bin", vec![1]).await.expect("put");
        store
            .put(&first, "z/other.bin", vec![2])
            .await
            .expect("put");
        store
            .put(&second, "a/file.bin", vec![3])
            .await
            .expect("put");
        assert_eq!(
            store.get(&first, "a/file.bin").await.expect("first get"),
            vec![1]
        );
        assert_eq!(
            store.get(&second, "a/file.bin").await.expect("second get"),
            vec![3]
        );
        assert_eq!(
            store.list(&first).await.expect("first list"),
            vec!["a/file.bin", "z/other.bin"]
        );
        assert_eq!(
            store.list(&second).await.expect("second list"),
            vec!["a/file.bin"]
        );
    }
}
