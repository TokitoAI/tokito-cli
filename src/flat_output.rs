//! Exclusive, manifest-last publication with conservative ordinary-error rollback.
//! This is not crash-atomic directory publication. No recursive cleanup is used.
use crate::{EngineError, sha256_hex, write_new_file};
use std::{fs, io::Read, path::Path};

fn unchanged(path: &Path, length: u64, digest: &str) -> bool {
    if !fs::symlink_metadata(path).is_ok_and(|m| m.is_file() && !linked(&m) && m.len() == length) {
        return false;
    }
    let Ok(file) = fs::File::open(path) else {
        return false;
    };
    let mut bytes = Vec::new();
    file.take(length.saturating_add(1))
        .read_to_end(&mut bytes)
        .is_ok()
        && bytes.len() as u64 == length
        && sha256_hex(&bytes) == digest
}

pub(crate) fn publish(output: &Path, files: &[(&str, Vec<u8>)]) -> Result<(), EngineError> {
    #[cfg(feature = "e2e-test-hooks")]
    {
        let fail_after = std::env::var("TOKITO_TWIN_TEST_FAIL_AFTER")
            .ok()
            .and_then(|value| value.parse::<usize>().ok());
        let mut written = 0;
        return publish_with(output, files, |path, bytes| {
            if fail_after == Some(written) {
                return Err(EngineError::new("injected twin publication failure"));
            }
            write_new_file(path, bytes)?;
            written += 1;
            Ok(())
        });
    }
    #[cfg(not(feature = "e2e-test-hooks"))]
    publish_with(output, files, write_new_file)
}

fn linked(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_type().is_symlink() || metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn publish_with(
    output: &Path,
    files: &[(&str, Vec<u8>)],
    mut write: impl FnMut(&Path, &[u8]) -> Result<(), EngineError>,
) -> Result<(), EngineError> {
    let mut names = std::collections::BTreeSet::new();
    for (name, _) in files {
        if name.is_empty()
            || name.contains(['/', '\\', ':'])
            || *name == "."
            || *name == ".."
            || !names.insert(*name)
        {
            return Err(EngineError::new("output requires unique flat filenames"));
        }
    }
    fs::create_dir(output)
        .map_err(|e| EngineError::new(format!("cannot create new output directory: {e}")))?;
    let mut completed: Vec<(std::path::PathBuf, u64, String)> = Vec::new();
    for (name, bytes) in files {
        let path = output.join(name);
        if let Err(error) = write(&path, bytes) {
            // Leave anything changed by another writer alone. Never follow links,
            // recursively delete a tree, or remove a nonempty directory.
            if fs::symlink_metadata(output).is_ok_and(|m| m.is_dir() && !linked(&m)) {
                for (path, length, digest) in completed {
                    if unchanged(&path, length, &digest) {
                        let _ = fs::remove_file(path);
                    }
                }
                let _ = fs::remove_dir(output);
            }
            return Err(EngineError::new(format!(
                "{error}; incomplete output was rolled back where unchanged; inspect '{}' if it remains (changed or unrelated files are preserved)",
                output.display()
            )));
        }
        completed.push((path, bytes.len() as u64, sha256_hex(bytes)));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rollback_preserves_competing_work_and_never_reuses_an_output() {
        let root = std::env::temp_dir().join(format!(
            "tokito-flat-output-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let files = [("body.glb", vec![1, 2]), ("manifest.json", vec![3])];
        let clean = root.join("clean");
        let mut writes = 0;
        assert!(
            publish_with(&clean, &files, |path, bytes| {
                writes += 1;
                if writes == 2 {
                    return Err(EngineError::new("injected storage failure"));
                }
                write_new_file(path, bytes)
            })
            .is_err()
        );
        assert!(!clean.exists());
        let competing = root.join("competing");
        let mut writes = 0;
        assert!(
            publish_with(&competing, &files, |path, bytes| {
                writes += 1;
                if writes == 2 {
                    fs::write(competing.join("body.glb"), b"XY").unwrap();
                    fs::write(competing.join("user.txt"), b"preserve").unwrap();
                    return Err(EngineError::new("injected storage failure"));
                }
                write_new_file(path, bytes)
            })
            .is_err()
        );
        assert_eq!(fs::read(competing.join("user.txt")).unwrap(), b"preserve");
        assert_eq!(fs::read(competing.join("body.glb")).unwrap(), b"XY");
        assert!(publish(&competing, &files).is_err());
        assert!(!competing.join("manifest.json").exists());
        for name in ["user.txt", "body.glb"] {
            fs::remove_file(competing.join(name)).unwrap();
        }
        fs::remove_dir(competing).unwrap();
        let success = root.join("success");
        publish(&success, &files).unwrap();
        for (name, bytes) in files {
            assert_eq!(fs::read(success.join(name)).unwrap(), bytes);
            fs::remove_file(success.join(name)).unwrap();
        }
        fs::remove_dir(success).unwrap();
        fs::remove_dir(root).unwrap();
    }
}
