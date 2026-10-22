use super::*;
use anyhow::bail;

fn last_stamp() -> &'static std::sync::Mutex<Option<String>> {
    static STAMP: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
    &STAMP
}

fn last_warnings() -> &'static std::sync::Mutex<Vec<String>> {
    static WARNINGS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    &WARNINGS
}

fn writable_content() -> Option<(PathBuf, PathBuf)> {
    let content = content_dir()?;
    let original = root().ok()?;
    let canonical_content = std::fs::canonicalize(&content).unwrap_or_else(|_| content.clone());
    let canonical_original = std::fs::canonicalize(&original).unwrap_or_else(|_| original.clone());
    (canonical_content != canonical_original).then_some((content, original))
}

fn relative(base: &Path, path: &Path) -> String {
    path.strip_prefix(base).unwrap_or(path).components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
}

fn install_unit(map_rel: &str) -> String {
    let mut parts = map_rel.split('/');
    match (parts.next(), parts.next()) {
        (Some(top), Some(archive)) if top.eq_ignore_ascii_case(install::ARCHIVES) => format!("{top}/{archive}"),
        _ => map_rel.to_string(),
    }
}

fn sync_stamp() -> String {
    let roots = omsi_cfg::content_roots();
    let mut stamp = index::content_stamp(&roots, None);
    let hof_dirs: Vec<PathBuf> = roots.iter().map(|r| r.join("HOFs")).collect();
    stamp.push_str(&format!("-{:016x}", index::folder_stamp(&hof_dirs)));
    stamp
}

fn is_hof(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("hof"))
}

fn hof_is_called(p: &Path, name: &str) -> bool {
    p.file_stem().is_some_and(|s| s.to_string_lossy().trim().eq_ignore_ascii_case(name)) || omsi_vehicle::Hof::read_name(p).is_some_and(|n| n.trim().eq_ignore_ascii_case(name))
}

fn depot_name(raw: &str) -> &str {
    let name = raw.trim();
    let name = name.rsplit(['\\', '/']).next().unwrap_or(name);
    match name.len().checked_sub(4) {
        Some(cut) if name.get(cut..).is_some_and(|ext| ext.eq_ignore_ascii_case(".hof")) => &name[..cut],
        _ => name,
    }
}

fn map_depot_source(map: &Path, name: &str) -> Option<PathBuf> {
    omsi_cfg::mirrored_dirs(map)
        .iter()
        .flat_map(|d| omsi_cfg::vfs::read_dir_paths(d))
        .find(|p| is_hof(p) && hof_is_called(p, name))
        .or_else(|| omsi_vehicle::hof::depot_anywhere(name).map(|h| h.path))
}

fn copy_to_shared(content: &Path, file_name: &str, bytes: &[u8]) -> Result<Option<String>> {
    let dir = content_case_path(content, "HOFs");
    if !path_stays_in_content(content, &dir) {
        bail!("the shared HOFs folder is outside the writable content folder");
    }
    std::fs::create_dir_all(&dir)?;
    let dest = content_case_path(content, &format!("HOFs/{file_name}"));
    let mut output = match std::fs::OpenOptions::new().write(true).create_new(true).open(&dest) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    if let Err(e) = std::io::Write::write_all(&mut output, bytes) {
        drop(output);
        let _ = std::fs::remove_file(&dest);
        return Err(e.into());
    }
    let rel = dest.strip_prefix(content).unwrap_or(&dest).components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
    Ok(Some(rel))
}

pub(super) fn sync_map_hofs() {
    let Some((content, original)) = writable_content() else { return };
    if install::jobs().iter().any(|j| j.finished.is_none()) {
        return;
    }
    let mut last = match last_stamp().try_lock() {
        Ok(last) => last,
        Err(std::sync::TryLockError::Poisoned(e)) => e.into_inner(),
        Err(std::sync::TryLockError::WouldBlock) => return,
    };
    let stamp = sync_stamp();
    if last.as_deref() == Some(stamp.as_str()) {
        return;
    }
    omsi_cfg::content_changed();

    let mut shared: Vec<PathBuf> = omsi_cfg::read_dir_merged("HOFs").into_iter().filter(|p| is_hof(p)).collect();
    let mut copied = 0usize;
    let mut warnings = Vec::new();
    for (folder, dirs) in merged_folders("maps") {
        let Some(map) = dirs.into_iter().find(|d| in_content(d) && !(original.starts_with(&content) && d.starts_with(&original)) && omsi_cfg::vfs::is_file(&d.join("global.cfg"))) else { continue };
        let ailists_path = map.join("ailists.cfg");
        if !omsi_cfg::vfs::is_file(&ailists_path) {
            continue;
        }
        let ailists = match omsi_map::ailists::AiLists::load(&ailists_path) {
            Ok(ailists) => ailists,
            Err(e) => {
                warnings.push(format!("{folder}: couldn't read ailists.cfg: {e}"));
                continue;
            }
        };
        let mut names: Vec<&str> = ailists.groups.iter().filter_map(|g| g.hof.as_deref()).map(depot_name).filter(|n| !n.is_empty()).collect();
        names.sort_by_key(|n| n.to_ascii_lowercase());
        names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        let mut added = Vec::new();
        for name in names {
            if shared.iter().any(|p| hof_is_called(p, name)) {
                continue;
            }
            let Some(source) = map_depot_source(&map, name) else {
                warnings.push(format!("{folder}: the HOF file '{name}.hof' it needs isn't installed."));
                continue;
            };
            let Some(file_name) = source.file_name().map(|n| n.to_string_lossy().to_string()) else { continue };
            let copy = omsi_cfg::vfs::read(&source).map_err(anyhow::Error::from).and_then(|bytes| copy_to_shared(&content, &file_name, &bytes));
            match copy {
                Ok(Some(rel)) => {
                    shared.push(content.join(&rel));
                    added.push(rel);
                    copied += 1;
                }
                Ok(None) => warnings.push(format!("{folder}: HOFs/{file_name} already holds another depot than '{name}'.")),
                Err(e) => warnings.push(format!("{folder}: couldn't share {file_name} with every bus: {e}")),
            }
        }
        if !added.is_empty() {
            mods::attach(&content, &install_unit(&relative(&content, &map)), &added);
        }
    }

    if copied > 0 {
        log_line(&format!("maps: shared {copied} map HOF file(s) with every bus in {}", content.join("HOFs").display()));
        omsi_cfg::content_changed();
    }
    warnings.sort();
    warnings.dedup();
    for warning in &warnings {
        log_line(&format!("maps: HOF sync warning: {warning}"));
    }
    *last_warnings().lock().unwrap_or_else(|e| e.into_inner()) = warnings;
    *last = (copied == 0).then_some(stamp);
}

pub fn add_shared_hofs(files: &[PathBuf]) -> Result<usize> {
    let Some((content, _)) = writable_content() else {
        bail!("the writable content folder is the original OMSI folder; HOF files were not copied");
    };
    if files.is_empty() {
        bail!("no HOF files were selected");
    }
    let mut copied = 0usize;
    let mut skipped = 0usize;
    let mut failures = Vec::new();
    for source in files {
        let Some(file_name) = source.file_name().map(|n| n.to_string_lossy().to_string()) else {
            failures.push(format!("{} has no file name", source.display()));
            continue;
        };
        if !is_hof(source) {
            failures.push(format!("{file_name} is not a .hof file"));
            continue;
        }
        match std::fs::read(source).map_err(anyhow::Error::from).and_then(|bytes| copy_to_shared(&content, &file_name, &bytes)) {
            Ok(Some(_)) => copied += 1,
            Ok(None) => skipped += 1,
            Err(e) => failures.push(format!("couldn't add {file_name}: {e}")),
        }
    }
    if copied > 0 {
        omsi_cfg::content_changed();
    }
    if !failures.is_empty() {
        bail!("added {copied} file(s), {skipped} already there; {}", failures.join("; "));
    }
    Ok(copied)
}

fn content_case_path(base: &Path, rel: &str) -> PathBuf {
    let mut current = base.to_path_buf();
    for component in rel.split('/').filter(|c| !c.is_empty()) {
        let direct = current.join(component);
        if direct.exists() {
            current = direct;
            continue;
        }
        let found = std::fs::read_dir(&current).ok().and_then(|rd| rd.flatten().map(|e| e.file_name()).find(|n| n.to_string_lossy().eq_ignore_ascii_case(component)));
        current = found.map(|name| current.join(name)).unwrap_or(direct);
    }
    current
}

fn path_stays_in_content(content: &Path, path: &Path) -> bool {
    let base = std::fs::canonicalize(content).unwrap_or_else(|_| content.to_path_buf());
    let mut ancestor = path;
    while !ancestor.exists() {
        let Some(parent) = ancestor.parent() else { return false };
        ancestor = parent;
    }
    std::fs::canonicalize(ancestor).map(|p| p.starts_with(base)).unwrap_or(false)
}

pub(super) fn warnings() -> Vec<String> {
    last_warnings().lock().unwrap_or_else(|e| e.into_inner()).clone()
}
