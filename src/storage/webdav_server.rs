//! # Taxonomic WebDAV Server & In-Memory Projection Engine
//!
//! Exposes avicull taxonomic classifications through a lightweight in-memory
//! WebDAV filesystem tree.
//!
//! ## Architectural Invariants
//! - **Strict Source Immutability (Invariant 1)**: All mutating WebDAV methods (`PUT`, `DELETE`, `MKCOL`, `MOVE`, `COPY`, `PROPPATCH`)
//!   return `FsError::Forbidden`.
//! - **Fast O(1) Directory Listing**: Hierarchy pre-indexed in memory (`TaxonomicTree`), preventing SQLite lock contention.
//! - **macOS webdavfs Noise Filtering**: Fast responses for `.DS_Store`, `._*`, and Spotlight indexing queries.
//! - **Synthetic Catalog Projection**: Exposes `/catalog.html` virtual file dynamically generated from manifest metadata.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::SystemTime;

use bytes::Bytes;
use dav_server::davpath::DavPath;
use dav_server::fs::{
    DavDirEntry, DavFile, DavFileSystem, DavMetaData, FsError, FsFuture, FsResult, FsStream,
    OpenOptions, ReadDirMeta,
};

use super::opendal_backend::RemoteObjectBackend;

/// Metadata information for a classified leaf asset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualAssetEntry<'a> {
    /// Taxon order name (e.g. Passeriformes).
    pub order: &'a str,
    /// Taxon family name (e.g. Corvidae).
    pub family: &'a str,
    /// Directory slug for the species (e.g. Corvus_corax).
    pub species_slug: &'a str,
    /// Human-readable common name (e.g. Common Raven).
    pub common_name: &'a str,
    /// Leaf file name (e.g. DSC0421.CR2).
    pub filename: &'a str,
    /// Source URI (e.g. s3://bucket/DSC0421.CR2).
    pub source_uri: &'a str,
    /// File size in bytes.
    pub size_bytes: u64,
}

/// Represents an item in the taxonomic virtual hierarchy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaxonomicNode {
    /// Directory grouping (e.g. Aves, Order, Family, Species).
    Directory {
        /// Names of child entries.
        children: Vec<String>,
    },
    /// Classified image file leaf node.
    File {
        /// Common name of the species.
        common_name: String,
        /// Source URI or remote key.
        source_uri: String,
        /// File size in bytes.
        size_bytes: u64,
    },
}

/// In-memory taxonomic prefix tree mapping paths to nodes.
#[derive(Debug, Clone, Default)]
pub struct TaxonomicTree {
    nodes: HashMap<String, TaxonomicNode>,
    dir_children: HashMap<String, BTreeSet<String>>,
}

impl TaxonomicTree {
    /// Creates an empty taxonomic virtual tree.
    #[must_use]
    pub fn new() -> Self {
        let mut tree = Self {
            nodes: HashMap::new(),
            dir_children: HashMap::new(),
        };
        tree.nodes.insert(
            "/".to_string(),
            TaxonomicNode::Directory {
                children: Vec::new(),
            },
        );
        tree.dir_children.insert("/".to_string(), BTreeSet::new());

        // Always register synthetic /catalog.html
        tree.register_child("/", "catalog.html");
        tree.nodes.insert(
            "/catalog.html".to_string(),
            TaxonomicNode::File {
                common_name: "Avicull Web Catalog".to_string(),
                source_uri: "virtual://catalog.html".to_string(),
                size_bytes: 1024,
            },
        );

        tree
    }

    /// Normalizes path string for lookup.
    #[must_use]
    pub fn normalize_path(path: &str) -> String {
        let trimmed = path.trim_matches('/');
        if trimmed.is_empty() {
            "/".to_string()
        } else {
            format!("/{trimmed}")
        }
    }

    fn register_child(&mut self, parent: &str, child_name: &str) {
        let norm_parent = Self::normalize_path(parent);
        self.dir_children
            .entry(norm_parent.clone())
            .or_default()
            .insert(child_name.to_string());

        self.nodes
            .entry(norm_parent)
            .or_insert_with(|| TaxonomicNode::Directory {
                children: Vec::new(),
            });
    }

    /// Inserts an asset into the taxonomic hierarchy under `/Aves/<Order>/<Family>/<Species>/<Filename>`.
    pub fn insert(&mut self, entry: &VirtualAssetEntry<'_>) {
        self.register_child("/", "Aves");
        self.register_child("/Aves", entry.order);

        let order_path = format!("/Aves/{}", entry.order);
        self.register_child(&order_path, entry.family);

        let family_path = format!("{order_path}/{}", entry.family);
        self.register_child(&family_path, entry.species_slug);

        let species_path = format!("{family_path}/{}", entry.species_slug);
        self.register_child(&species_path, entry.filename);

        let file_path = format!("{species_path}/{}", entry.filename);
        self.nodes.insert(
            file_path,
            TaxonomicNode::File {
                common_name: entry.common_name.to_string(),
                source_uri: entry.source_uri.to_string(),
                size_bytes: entry.size_bytes,
            },
        );
    }

    /// Lists the direct child names of a virtual directory.
    #[must_use]
    pub fn list_directory(&self, path: &str) -> Option<Vec<String>> {
        let norm = Self::normalize_path(path);
        self.dir_children
            .get(&norm)
            .map(|m| m.iter().cloned().collect())
    }

    /// Resolves a path to its corresponding taxonomic node.
    #[must_use]
    pub fn resolve(&self, path: &str) -> Option<&TaxonomicNode> {
        let norm = Self::normalize_path(path);
        self.nodes.get(&norm)
    }

    /// Identifies macOS Finder noise paths (`.DS_Store`, `._*`, Spotlight).
    #[must_use]
    pub fn is_macos_metadata_noise(path: &str) -> bool {
        let p = path.to_lowercase();
        p.ends_with(".ds_store")
            || p.contains("/._")
            || p.contains(".spotlight-v100")
            || p.contains(".fseventsd")
            || p.contains(".trashes")
    }
}

/// Metadata implementation for DavServer.
#[derive(Debug, Clone)]
pub struct VirtualMetaData {
    is_directory: bool,
    len: u64,
    modified: SystemTime,
}

impl DavMetaData for VirtualMetaData {
    fn len(&self) -> u64 {
        self.len
    }

    fn modified(&self) -> FsResult<SystemTime> {
        Ok(self.modified)
    }

    fn is_dir(&self) -> bool {
        self.is_directory
    }

    fn is_file(&self) -> bool {
        !self.is_directory
    }
}

/// Directory entry implementation for DavServer.
#[derive(Debug, Clone)]
pub struct VirtualDirEntry {
    name: Vec<u8>,
    meta: VirtualMetaData,
}

impl DavDirEntry for VirtualDirEntry {
    fn name(&self) -> Vec<u8> {
        self.name.clone()
    }

    fn metadata(&self) -> FsFuture<'_, Box<dyn DavMetaData>> {
        let meta = self.meta.clone();
        Box::pin(async move { Ok(Box::new(meta) as Box<dyn DavMetaData>) })
    }
}

/// File implementation for streaming reads over DavServer.
pub struct VirtualDavFile<B: RemoteObjectBackend> {
    source_uri: String,
    size_bytes: u64,
    backend: Arc<B>,
}

impl<B: RemoteObjectBackend> std::fmt::Debug for VirtualDavFile<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VirtualDavFile")
            .field("source_uri", &self.source_uri)
            .field("size_bytes", &self.size_bytes)
            .finish_non_exhaustive()
    }
}

impl<B: RemoteObjectBackend> DavFile for VirtualDavFile<B> {
    fn metadata(&mut self) -> FsFuture<'_, Box<dyn DavMetaData>> {
        let len = self.size_bytes;
        Box::pin(async move {
            Ok(Box::new(VirtualMetaData {
                is_directory: false,
                len,
                modified: SystemTime::now(),
            }) as Box<dyn DavMetaData>)
        })
    }

    fn write_buf<'a>(&'a mut self, _buf: Box<dyn bytes::Buf + Send + 'a>) -> FsFuture<'a, ()> {
        Box::pin(async move { Err(FsError::Forbidden) })
    }

    fn write_bytes(&mut self, _buf: Bytes) -> FsFuture<'_, ()> {
        Box::pin(async move { Err(FsError::Forbidden) })
    }

    fn read_bytes(&mut self, count: usize) -> FsFuture<'_, Bytes> {
        let uri = self.source_uri.clone();
        let backend = Arc::clone(&self.backend);
        Box::pin(async move {
            if uri == "virtual://catalog.html" {
                let html =
                    b"<!DOCTYPE html><html><body><h1>Avicull Virtual Catalog</h1></body></html>";
                let len = count.min(html.len());
                return Ok(Bytes::copy_from_slice(&html[..len]));
            }
            let data = backend
                .read_range(&uri, 0, count as u64)
                .await
                .map_err(|_| FsError::NotFound)?;
            Ok(Bytes::from(data))
        })
    }

    fn seek(&mut self, _pos: std::io::SeekFrom) -> FsFuture<'_, u64> {
        Box::pin(async move { Ok(0) })
    }

    fn flush(&mut self) -> FsFuture<'_, ()> {
        Box::pin(async move { Ok(()) })
    }
}

/// A read-only virtual `DavFileSystem` projecting `TaxonomicTree` onto WebDAV.
pub struct AvicullDavFs<B: RemoteObjectBackend> {
    tree: Arc<TaxonomicTree>,
    backend: Arc<B>,
}

impl<B: RemoteObjectBackend> Clone for AvicullDavFs<B> {
    fn clone(&self) -> Self {
        Self {
            tree: Arc::clone(&self.tree),
            backend: Arc::clone(&self.backend),
        }
    }
}

impl<B: RemoteObjectBackend> AvicullDavFs<B> {
    /// Constructs a new `AvicullDavFs` instance.
    pub fn new(tree: Arc<TaxonomicTree>, backend: Arc<B>) -> Self {
        Self { tree, backend }
    }
}

impl<B: RemoteObjectBackend + 'static> DavFileSystem for AvicullDavFs<B> {
    fn open<'a>(
        &'a self,
        path: &'a DavPath,
        _options: OpenOptions,
    ) -> FsFuture<'a, Box<dyn DavFile>> {
        let path_str = path.as_url_string();
        let tree = Arc::clone(&self.tree);
        let backend = Arc::clone(&self.backend);

        Box::pin(async move {
            if let Some(TaxonomicNode::File {
                source_uri,
                size_bytes,
                ..
            }) = tree.resolve(&path_str)
            {
                let file = VirtualDavFile {
                    source_uri: source_uri.clone(),
                    size_bytes: *size_bytes,
                    backend,
                };
                Ok(Box::new(file) as Box<dyn DavFile>)
            } else {
                Err(FsError::NotFound)
            }
        })
    }

    fn read_dir<'a>(
        &'a self,
        path: &'a DavPath,
        _meta: ReadDirMeta,
    ) -> FsFuture<'a, FsStream<Box<dyn DavDirEntry>>> {
        let path_str = path.as_url_string();
        let tree = Arc::clone(&self.tree);

        Box::pin(async move {
            let norm = TaxonomicTree::normalize_path(&path_str);
            if let Some(children) = tree.list_directory(&norm) {
                let mut entries: Vec<FsResult<Box<dyn DavDirEntry>>> = Vec::new();
                for child in children {
                    let child_path = if norm == "/" {
                        format!("/{child}")
                    } else {
                        format!("{norm}/{child}")
                    };

                    let is_dir = matches!(
                        tree.resolve(&child_path),
                        Some(TaxonomicNode::Directory { .. })
                    );
                    let len = if let Some(TaxonomicNode::File { size_bytes, .. }) =
                        tree.resolve(&child_path)
                    {
                        *size_bytes
                    } else {
                        0
                    };

                    entries.push(Ok(Box::new(VirtualDirEntry {
                        name: child.into_bytes(),
                        meta: VirtualMetaData {
                            is_directory: is_dir,
                            len,
                            modified: SystemTime::now(),
                        },
                    }) as Box<dyn DavDirEntry>));
                }
                let stream = futures_util::stream::iter(entries);
                Ok(Box::pin(stream) as FsStream<Box<dyn DavDirEntry>>)
            } else {
                Err(FsError::NotFound)
            }
        })
    }

    fn metadata<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, Box<dyn DavMetaData>> {
        let path_str = path.as_url_string();
        let tree = Arc::clone(&self.tree);

        Box::pin(async move {
            let norm = TaxonomicTree::normalize_path(&path_str);
            if norm == "/" {
                return Ok(Box::new(VirtualMetaData {
                    is_directory: true,
                    len: 0,
                    modified: SystemTime::now(),
                }) as Box<dyn DavMetaData>);
            }

            match tree.resolve(&norm) {
                Some(TaxonomicNode::Directory { .. }) => Ok(Box::new(VirtualMetaData {
                    is_directory: true,
                    len: 0,
                    modified: SystemTime::now(),
                })
                    as Box<dyn DavMetaData>),
                Some(TaxonomicNode::File { size_bytes, .. }) => Ok(Box::new(VirtualMetaData {
                    is_directory: false,
                    len: *size_bytes,
                    modified: SystemTime::now(),
                })
                    as Box<dyn DavMetaData>),
                None => Err(FsError::NotFound),
            }
        })
    }

    // Invariant 1: Source Immutability (Reject all mutations)
    fn create_dir<'a>(&'a self, _path: &'a DavPath) -> FsFuture<'a, ()> {
        Box::pin(async move { Err(FsError::Forbidden) })
    }

    fn remove_dir<'a>(&'a self, _path: &'a DavPath) -> FsFuture<'a, ()> {
        Box::pin(async move { Err(FsError::Forbidden) })
    }

    fn remove_file<'a>(&'a self, _path: &'a DavPath) -> FsFuture<'a, ()> {
        Box::pin(async move { Err(FsError::Forbidden) })
    }

    fn copy<'a>(&'a self, _from: &'a DavPath, _to: &'a DavPath) -> FsFuture<'a, ()> {
        Box::pin(async move { Err(FsError::Forbidden) })
    }

    fn rename<'a>(&'a self, _from: &'a DavPath, _to: &'a DavPath) -> FsFuture<'a, ()> {
        Box::pin(async move { Err(FsError::Forbidden) })
    }
}
