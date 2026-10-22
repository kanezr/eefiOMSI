//! Our own bus as the others are to know it: the files it is made of, and their fingerprint.

use super::*;

/// Our own bus as the others are to know it: its files and their fingerprint, worked out
/// once for each bus and paint (`identify`, `begin`). The list stays as it was made, so the
/// fingerprint does not change under the others while the game reads on; what is read later
/// that the list lacks is a miss of `bus_files` to be found, not a new bus.
pub(super) struct Own {
    key: String,
    manifest: crate::mods::identity::VehicleManifest,
    fingerprint: String,
    files: Vec<(String, PathBuf)>,
}

/// Our bus's list, its fingerprint and its files (where each is read from).
pub(in crate::mods) type OwnIdentity = (crate::mods::identity::VehicleManifest, String, Vec<(String, PathBuf)>);

/// Our bus's identity, worked out once for each bus and paint. The game has one (`own_bus`,
/// behind this module's functions).
#[derive(Default)]
pub(in crate::mods) struct OwnBus {
    own: Mutex<Option<Own>>,
    /// (key, fingerprint) of `own`, for the frame: it never waits for one being worked out.
    fp: Mutex<(String, String)>,
    identifying: AtomicBool,
    /// The bus and paint whose files made no valid list: not worked out again every frame
    /// (only when the bus or its paint changes).
    failed: Mutex<String>,
}

static OWN_BUS: std::sync::LazyLock<OwnBus> = std::sync::LazyLock::new(OwnBus::default);

/// The game's own `OwnBus`.
pub(in crate::mods) fn own_bus() -> &'static OwnBus {
    &OWN_BUS
}

/// The fingerprint of our bus in that paint, once it is known (else empty).
pub(crate) fn own_fingerprint(bus: &str, paint: &str) -> String {
    own_bus().fingerprint(bus, paint)
}

/// Once a frame: work out our bus's fingerprint in the background when the bus or its
/// paint is new. `own` lists the bus's files (`own_files`): it runs on that thread too.
pub(crate) fn identify(root: &Path, bus: &str, paint: &str, own: impl FnOnce() -> Vec<PathBuf> + Send + 'static) {
    own_bus().identify(root, bus, paint, own);
}

/// Our bus's manifest in that paint, worked out if it is not yet (only then are its files
/// listed, `own`). None when the bus's files make no valid list.
pub(super) fn own_manifest(root: &Path, bus: &str, paint: &str, own: impl FnOnce() -> Vec<PathBuf>) -> Option<OwnIdentity> {
    own_bus().manifest(root, bus, paint, own)
}

impl OwnBus {
    pub(in crate::mods) fn fingerprint(&self, bus: &str, paint: &str) -> String {
        let fp = self.fp.lock().unwrap_or_else(|e| e.into_inner());
        if fp.0 == format!("{bus}|{paint}") {
            fp.1.clone()
        } else {
            String::new()
        }
    }

    pub(in crate::mods) fn identify(&'static self, root: &Path, bus: &str, paint: &str, own: impl FnOnce() -> Vec<PathBuf> + Send + 'static) {
        if bus.is_empty() || self.identifying.load(Ordering::Relaxed) {
            return;
        }
        let key = format!("{bus}|{paint}");
        if self.fp.lock().unwrap_or_else(|e| e.into_inner()).0 == key || *self.failed.lock().unwrap_or_else(|e| e.into_inner()) == key {
            return;
        }
        self.identifying.store(true, Ordering::Relaxed);
        let (root, bus, paint) = (root.to_path_buf(), bus.to_string(), paint.to_string());
        // (the files are listed on the thread: `bus_files` looks up sounds, textures and fonts)
        let spawned = std::thread::Builder::new().name("bus-identity".into()).spawn(move || {
            self.manifest(&root, &bus, &paint, own);
            self.identifying.store(false, Ordering::Relaxed);
        });
        if spawned.is_err() {
            self.identifying.store(false, Ordering::Relaxed);
        }
    }

    pub(in crate::mods) fn manifest(&self, root: &Path, bus: &str, paint: &str, own: impl FnOnce() -> Vec<PathBuf>) -> Option<OwnIdentity> {
        let key = format!("{bus}|{paint}");
        let mut guard = self.own.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(o) = guard.as_ref().filter(|o| o.key == key) {
            return Some((o.manifest.clone(), o.fingerprint.clone(), o.files.clone()));
        }
        let files = chosen(root, bus, own());
        // (what this run read was only for `chosen`'s log of what the description misses: the
        // recording stops and its set goes - it was kept, and grew, for the whole session)
        omsi_cfg::vfs::record_reads(false);
        let bus_rel = omsi_net::vehicle_path(bus).unwrap_or_else(|| bus.to_string());
        let made = crate::mods::identity::VehicleManifest { bus: bus_rel, entries: hashed(&files) }.validated();
        let manifest = match made {
            Ok(m) => m,
            Err(e) => {
                log::warn!("bus identity: {bus}: {e}");
                *self.failed.lock().unwrap_or_else(|e| e.into_inner()) = key;
                return None;
            }
        };
        let fingerprint = manifest.fingerprint();
        log::info!("bus identity: {bus} ({paint}) is {fingerprint}: {} files, {:.1} MB", manifest.entries.len(), manifest.size() as f64 / 1e6);
        *self.fp.lock().unwrap_or_else(|e| e.into_inner()) = (key.clone(), fingerprint.clone());
        self.failed.lock().unwrap_or_else(|e| e.into_inner()).clear();
        *guard = Some(Own { key, manifest: manifest.clone(), fingerprint: fingerprint.clone(), files: files.clone() });
        Some((manifest, fingerprint, files))
    }

    /// The bus worked out last is forgotten (it is worked out again).
    #[cfg(test)]
    pub(in crate::mods) fn forget(&self) {
        *self.own.lock().unwrap_or_else(|e| e.into_inner()) = None;
        self.failed.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    /// Whether the files of that bus and paint made no valid list.
    #[cfg(test)]
    pub(in crate::mods) fn has_failed(&self, bus: &str, paint: &str) -> bool {
        *self.failed.lock().unwrap_or_else(|e| e.into_inner()) == format!("{bus}|{paint}")
    }
}

/// What goes for `bus` - and what its fingerprint is over: the files its description names
/// (`bus_files`, `own`), each once by the path every machine knows it by. Only those: any
/// machine with the same files works out the same list, so a player who has the same version
/// of the bus knows it is the same without fetching anything (`vehicles::want`). What this
/// game read for the bus besides is no part of it: it is logged, a miss of `bus_files` to
/// look into.
pub(super) fn chosen(root: &Path, bus: &str, own: Vec<PathBuf>) -> Vec<(String, PathBuf)> {
    let out = listed(root, own);
    let named: HashSet<String> = out.iter().filter_map(|(rel, _)| crate::mods::identity::key(rel)).collect();
    let extra: Vec<String> = files_of(root, bus).into_iter().map(|(rel, _)| rel).filter(|rel| crate::mods::identity::key(rel).is_some_and(|k| !named.contains(&k))).collect();
    if !extra.is_empty() {
        log::info!("bus identity: {bus}: {} files were read for it that its description does not name (not in its fingerprint), e.g. {}", extra.len(), extra.iter().take(8).cloned().collect::<Vec<_>>().join(", "));
    }
    out
}

/// `own` (absolute) as (path every machine knows it by, where it is), each once, in order -
/// once by the key the list is checked by (`identity::key`: case and Unicode form aside), so
/// one file reached by two spellings (NFC and NFD, as macOS may give) is listed once.
pub(crate) fn listed(root: &Path, own: Vec<PathBuf>) -> Vec<(String, PathBuf)> {
    let mut seen = HashSet::new();
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    for p in own {
        if let Some(rel) = crate::mods::bus_files::relative(root, &p).filter(|r| crate::mods::refuse_path(r).is_none()) {
            if crate::mods::identity::key(&rel).is_some_and(|k| seen.insert(k)) {
                out.push((rel, p));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// The files of the bus `v` the player drives (None: no bus, no files), in its paint
/// (`bus_files`), listed when the answer is called - on the thread that needs them, not the
/// frame's: `bus_files` looks up sound configs, the textures of every material and the
/// fonts. Taken now is only what that needs (the vehicle's types are shared, not copied).
/// (Not its depot file: which one a player picked is no part of the bus, and the displays'
/// texts go along in INFO.)
pub fn own_files(root: &Path, v: Option<&omsi_sim::VehicleInstance>) -> impl FnOnce() -> Vec<PathBuf> + Send + 'static {
    let root = root.to_path_buf();
    // (its rear sections as the vehicle has them loaded: not read from the disk again)
    let taken = v.map(|v| (v.ty.clone(), v.trailers.iter().map(|t| t.ty.clone()).collect::<Vec<_>>(), v.host.paint_scheme.flatten()));
    move || {
        let Some((ty, trailers, scheme)) = taken else { return Vec::new() };
        let parts: Vec<&omsi_sim::VehicleType> = trailers.iter().map(|t| &**t).collect();
        crate::mods::bus_files::of_parts(&root, &ty, &parts, scheme, None)
    }
}

/// The files of ours this game has read for `bus`: its folder and the folders its texts
/// point into, and the fonts its text textures use. (relative path, where it is read from)
pub(super) fn files_of(root: &Path, bus: &str) -> Vec<(String, PathBuf)> {
    let Some(bus_rel) = omsi_net::vehicle_path(bus).map(|b| norm(&b.replace('\\', "/"))) else { return Vec::new() };
    // our own content (not the original installation, not what the host sent)
    let roots: Vec<PathBuf> = omsi_cfg::content_roots().into_iter().filter(|r| r != root && !omsi_cfg::is_sandbox(r)).collect();
    let mut read: HashMap<String, (String, PathBuf)> = HashMap::new();
    for p in omsi_cfg::vfs::reads() {
        let Some(r) = roots.iter().filter(|r| p.starts_with(r)).max_by_key(|r| r.components().count()) else { continue };
        let Ok(rest) = p.strip_prefix(r) else { continue };
        let rel: Vec<String> = rest.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
        let rel = rel.join("/");
        read.insert(rel.to_lowercase(), (rel, p));
    }
    let Some(own) = owner_folder(&bus_rel) else { return Vec::new() };
    let mut prefixes: HashSet<String> = HashSet::from([own.to_lowercase()]);
    let inside = |prefixes: &HashSet<String>, lower: &str| prefixes.iter().any(|f| lower.starts_with(&format!("{f}/")));
    let mut scanned: HashSet<String> = HashSet::new();
    let mut fonts_used: HashSet<String> = HashSet::new();
    for _ in 0..6 {
        let before = prefixes.len();
        let texts: Vec<(String, PathBuf)> = read
            .iter()
            .filter(|(lower, _)| inside(&prefixes, lower) && !scanned.contains(*lower))
            .filter(|(lower, _)| [".bus", ".cfg", ".osc", ".txt", ".ovh", ".zug"].iter().any(|e| lower.ends_with(e)))
            .map(|(lower, (rel, p))| {
                let _ = lower;
                (rel.clone(), p.clone())
            })
            .collect();
        for (rel, path) in texts {
            scanned.insert(rel.to_lowercase());
            let Ok(text) = omsi_cfg::vfs::read_text(&path) else { continue };
            let here = rel.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
            let lines: Vec<&str> = text.lines().collect();
            for (i, l) in lines.iter().enumerate() {
                let t = l.trim();
                if t.to_ascii_lowercase().starts_with("[texttexture") {
                    if let Some(f) = lines.get(i + 2) {
                        fonts_used.insert(f.trim().to_lowercase());
                    }
                }
                if !t.contains("..") || t.starts_with('[') || t.len() > 300 {
                    continue;
                }
                let target = norm(&format!("{here}/{t}"));
                let top = target.split('/').next().unwrap_or("").to_ascii_lowercase();
                if !crate::mods::FOLDERS.contains(&top.as_str()) {
                    continue;
                }
                if let Some(f) = owner_folder(&target).filter(|f| f.split('/').count() >= 2) {
                    prefixes.insert(f.to_lowercase());
                }
            }
        }
        if prefixes.len() == before {
            break;
        }
    }
    let mut out: Vec<(String, PathBuf)> = read.iter().filter(|(lower, _)| inside(&prefixes, lower)).map(|(_, v)| v.clone()).collect();
    // the fonts: each `.oft` read with a `[newfont]` of a name in use, and its bitmaps
    for (lower, (rel, path)) in &read {
        if !lower.starts_with("fonts/") || !lower.ends_with(".oft") {
            continue;
        }
        let Ok(text) = omsi_cfg::vfs::read_text(path) else { continue };
        let lines: Vec<&str> = text.lines().map(|l| l.trim()).collect();
        let mut wanted = false;
        for (i, l) in lines.iter().enumerate() {
            if l.eq_ignore_ascii_case("[newfont]") && lines.get(i + 1).is_some_and(|n| fonts_used.contains(&n.to_lowercase())) {
                wanted = true;
                for b in lines.iter().skip(i + 2).take(2) {
                    if let Some(v) = read.get(&format!("fonts/{}", b.to_lowercase())) {
                        out.push(v.clone());
                    }
                }
            }
        }
        if wanted {
            out.push((rel.clone(), path.clone()));
        }
    }
    let mut seen = HashSet::new();
    out.retain(|(rel, _)| crate::mods::refuse_path(rel).is_none() && seen.insert(rel.to_lowercase()));
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// A local file's hash as remembered (`crate::mods::hash_cache`): its key, then (size,
/// modification time, SHA-256).
type Remembered = (String, (u64, u64, String));

/// Size and SHA-256 of each file (the hashes remembered by size and time, as a join does).
pub(in crate::mods) fn hashed(files: &[(String, PathBuf)]) -> Vec<Entry> {
    use rayon::prelude::*;
    let cache = crate::mods::hash_cache();
    let done: Vec<(Entry, Option<Remembered>)> = files
        .par_iter()
        .filter_map(|(rel, p)| {
            let key = p.to_string_lossy().to_string();
            if let Ok(md) = std::fs::metadata(p) {
                let mtime = md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
                if let Some((sz, mt, h)) = cache.get(&key) {
                    if *sz == md.len() && *mt == mtime {
                        // (checked as a file read now is: the hashes are remembered for every
                        // file a join compares, a program or one too large among them)
                        if md.len() > crate::mods::MAX_FILE || starts_as_program(p) {
                            return None;
                        }
                        return Some((Entry { path: rel.clone(), size: md.len(), sha256: h.clone() }, None));
                    }
                }
            }
            let data = omsi_cfg::vfs::read(p).ok()?;
            if data.len() as u64 > crate::mods::MAX_FILE || crate::mods::looks_executable(&data[..data.len().min(8)]) {
                return None;
            }
            let h = crate::mods::sha256_of(&data);
            let fresh = std::fs::metadata(p).ok().map(|md| {
                let mtime = md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
                (key, (md.len(), mtime, h.clone()))
            });
            Some((Entry { path: rel.clone(), size: data.len() as u64, sha256: h }, fresh))
        })
        .collect();
    let mut cache = cache;
    let mut entries = Vec::with_capacity(done.len());
    for (e, fresh) in done {
        if let Some((k, v)) = fresh {
            cache.insert(k, v);
        }
        entries.push(e);
    }
    crate::mods::save_hash_cache(&cache);
    entries
}

/// Does the plain file `p` start as a program does (`looks_executable`)? One that cannot be
/// read is taken for one.
pub(super) fn starts_as_program(p: &Path) -> bool {
    let mut head = [0u8; 8];
    let Ok(mut f) = std::fs::File::open(p) else { return true };
    let mut n = 0;
    while n < head.len() {
        match f.read(&mut head[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(_) => return true,
        }
    }
    crate::mods::looks_executable(&head[..n])
}

/// `a/b/../c` → `a/c`, `/`-separated.
pub(super) fn norm(p: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for c in p.split(['/', '\\']) {
        match c.trim() {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            c => parts.push(c),
        }
    }
    parts.join("/")
}

/// The folder a file belongs to: a vehicle's whole folder (`Vehicles/<name>`), else the
/// folder the file is in.
pub(super) fn owner_folder(rel: &str) -> Option<String> {
    let comps: Vec<&str> = rel.split('/').collect();
    if comps.len() < 3 {
        return None;
    }
    let top = comps[0].to_ascii_lowercase();
    if top == "vehicles" || top == "trains" {
        return Some(comps[..2].join("/"));
    }
    Some(comps[..comps.len() - 1].join("/"))
}
