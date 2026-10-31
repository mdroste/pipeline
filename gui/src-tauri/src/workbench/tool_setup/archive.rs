//! Extract only the pinned Node executable, npm, and notices into fresh staging.
//! Links and special files are never materialized; npm is invoked by its JS path.
use std::{
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

fn destination(path: &Path) -> Result<Option<PathBuf>, String> {
    if !path.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err("Unsafe path in command runtime archive".into());
    }
    let relative: PathBuf = path.components().skip(1).collect();
    let text = relative.to_string_lossy();
    Ok((text == "bin/node"
        || text == "node.exe"
        || text == "LICENSE"
        || text.starts_with("lib/node_modules/npm/")
        || text.starts_with("node_modules/npm/"))
    .then_some(relative))
}

fn write_file(
    root: &Path,
    relative: &Path,
    input: &mut impl Read,
    size: u64,
    total: &mut u64,
) -> Result<(), String> {
    *total = total
        .checked_add(size)
        .ok_or("Runtime archive size overflow")?;
    if *total > 512 * 1024 * 1024 {
        return Err("Runtime archive exceeds 512 MB".into());
    }
    let path = root.join(relative);
    fs::create_dir_all(path.parent().ok_or("Invalid runtime path")?).map_err(|e| e.to_string())?;
    let mut out = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    let copied = std::io::copy(&mut input.take(size + 1), &mut out).map_err(|e| e.to_string())?;
    if copied != size {
        return Err("Runtime archive entry size mismatch".into());
    }
    out.flush().map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            &path,
            fs::Permissions::from_mode(if relative == Path::new("bin/node") {
                0o700
            } else {
                0o600
            }),
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(super) fn unpack(archive: &Path, root: &Path, zip: bool) -> Result<(), String> {
    let file = crate::safety::open_regular_file(archive).map_err(|e| e.to_string())?;
    let mut total = 0;
    if zip {
        let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
        if archive.len() > 30_000 {
            return Err("Runtime archive has too many entries".into());
        }
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
            let path = entry.enclosed_name().ok_or("Unsafe runtime archive path")?;
            let relative = destination(&path)?;
            if !entry.is_file() || entry.is_symlink() {
                continue;
            }
            if let Some(relative) = relative {
                let size = entry.size();
                write_file(root, &relative, &mut entry, size, &mut total)?;
            }
        }
    } else {
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(file));
        for (i, entry) in archive.entries().map_err(|e| e.to_string())?.enumerate() {
            if i >= 30_000 {
                return Err("Runtime archive has too many entries".into());
            }
            let mut entry = entry.map_err(|e| e.to_string())?;
            let relative = destination(&entry.path().map_err(|e| e.to_string())?)?;
            if !entry.header().entry_type().is_file() {
                continue;
            }
            if let Some(relative) = relative {
                let size = entry.size();
                write_file(root, &relative, &mut entry, size, &mut total)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_runtime_and_npm_are_selected_and_paths_cannot_escape() {
        assert!(destination(Path::new("node/../../escape")).is_err());
        assert!(destination(Path::new("/node/bin/node")).is_err());
        assert_eq!(
            destination(Path::new("node/bin/node")).unwrap(),
            Some("bin/node".into())
        );
        assert!(destination(Path::new("node/include/node/header.h"))
            .unwrap()
            .is_none());
    }
    #[test]
    fn extraction_refuses_overwrite_and_oversized_entries() {
        let dir = tempfile::tempdir().unwrap();
        write_file(
            dir.path(),
            Path::new("node.exe"),
            &mut &b"node"[..],
            4,
            &mut 0,
        )
        .unwrap();
        assert!(write_file(
            dir.path(),
            Path::new("node.exe"),
            &mut &b"bad"[..],
            3,
            &mut 0
        )
        .is_err());
        assert!(write_file(
            dir.path(),
            Path::new("big"),
            &mut &b""[..],
            513 * 1024 * 1024,
            &mut 0
        )
        .is_err());
        assert_eq!(fs::read(dir.path().join("node.exe")).unwrap(), b"node");
    }
}
