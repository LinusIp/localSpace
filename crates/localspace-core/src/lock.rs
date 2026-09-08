//! `environment.lock` (spec §17.2).
//!
//! Every installed package, its exact version, a content hash of its files, the
//! source it came from, and the interfaces it provides. It is a document in the
//! DAG, so a change to the environment is a diff, and replaying it on another
//! machine or for another employee installs the same bytes.

use crate::registry::Registry;
use serde::{Deserialize, Serialize};
use serde_json::Value as J;
use std::path::Path;

/// The document id the lock lives under.
pub const LOCK_DOC: &str = "environment_lock";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockEntry {
    pub id: String,
    pub version: String,
    pub kind: String,
    /// blake3 over the package's files, names included, in a fixed order.
    pub hash: String,
    /// The directory the package was installed from.
    pub source: String,
    pub interfaces: Vec<String>,
}

/// The lock as a JSON document, sorted by id so two environments with the same
/// packages produce byte-identical locks.
pub fn compute(registry: &Registry) -> J {
    let mut packages: Vec<LockEntry> = registry
        .iter()
        .map(|h| LockEntry {
            id: h.manifest.harness.id.clone(),
            version: h.manifest.harness.version.clone(),
            kind: h.manifest.package.kind.label().to_string(),
            hash: package_hash(&h.dir),
            source: h
                .dir
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            interfaces: h.manifest.provides.interfaces.clone(),
        })
        .collect();
    packages.sort_by(|a, b| a.id.cmp(&b.id));
    serde_json::json!({
        "version": 1,
        "packages": packages,
    })
}

/// Content hash of a package: every regular file under its directory, two
/// levels deep, hashed as `name\0bytes` in sorted order. The same files in a
/// different directory hash the same, which is what makes mirrors and offline
/// bundles byte-identical to the registry.
pub fn package_hash(dir: &Path) -> String {
    let mut files: Vec<(String, std::path::PathBuf)> = Vec::new();
    collect(dir, "", &mut files, 0);
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut hasher = blake3::Hasher::new();
    for (name, path) in files {
        hasher.update(name.as_bytes());
        hasher.update(&[0]);
        if let Ok(bytes) = std::fs::read(&path) {
            hasher.update(&bytes);
        }
        hasher.update(&[0]);
    }
    hasher.finalize().to_hex().to_string()
}

fn collect(dir: &Path, prefix: &str, out: &mut Vec<(String, std::path::PathBuf)>, depth: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if path.is_dir() {
            if depth < 2 {
                collect(&path, &rel, out, depth + 1);
            }
        } else {
            out.push((rel, path));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_files_hash_the_same_wherever_they_live() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        for dir in [a.path(), b.path()] {
            std::fs::write(dir.join("harness.toml"), "x = 1").unwrap();
            std::fs::create_dir_all(dir.join("ui")).unwrap();
            std::fs::write(dir.join("ui/board.wasm"), b"\0asm").unwrap();
        }
        assert_eq!(package_hash(a.path()), package_hash(b.path()));

        std::fs::write(b.path().join("harness.toml"), "x = 2").unwrap();
        assert_ne!(package_hash(a.path()), package_hash(b.path()));
    }

    #[test]
    fn an_empty_registry_locks_to_no_packages() {
        let lock = compute(&Registry::new());
        assert_eq!(lock["version"], 1);
        assert_eq!(lock["packages"].as_array().unwrap().len(), 0);
    }
}
