use std::path::{Component, Path, PathBuf};

fn is_linked(metadata: &std::fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    false
}

fn assert_no_linked_components(path: &Path) {
    for component in path.ancestors() {
        match std::fs::symlink_metadata(component) {
            Ok(metadata) => assert!(
                !is_linked(&metadata),
                "test scratch path contains a linked component: {}",
                component.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!(
                "inspect test scratch path component {}: {error}",
                component.display()
            ),
        }
    }
}

fn normalize_for_test_path(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(path.to_string_lossy().trim_start_matches(r"\\?\"))
    }
    #[cfg(not(windows))]
    {
        path
    }
}

pub fn directory(name: &str) -> PathBuf {
    let base = std::env::var_os("TOKITO_E2E_TEMP_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target"));
    assert!(base.is_absolute(), "test scratch root must be absolute");
    let requested = Path::new(name);
    assert!(
        matches!(
            requested.components().collect::<Vec<_>>().as_slice(),
            [Component::Normal(_)]
        ),
        "test scratch directory name must be one path component"
    );
    assert_no_linked_components(&base);
    std::fs::create_dir_all(&base).expect("create test scratch root");
    assert_no_linked_components(&base);
    let canonical_base =
        normalize_for_test_path(base.canonicalize().expect("canonical test scratch root"));
    let path = base.join(requested);
    assert_no_linked_components(&path);
    std::fs::create_dir_all(&path).expect("create isolated test scratch directory");
    assert_no_linked_components(&path);
    let canonical = normalize_for_test_path(
        path.canonicalize()
            .expect("canonical test scratch directory"),
    );
    assert!(
        canonical.starts_with(&canonical_base),
        "test scratch directory escaped its selected root"
    );
    canonical
}

#[test]
fn rejects_multi_component_scratch_names() {
    assert!(std::panic::catch_unwind(|| directory("../escape")).is_err());
}
