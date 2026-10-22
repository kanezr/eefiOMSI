//! Opt-in real weather: map groups select a location, never a credential.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[cfg(test)]
mod storage_tests;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapConfig {
    version: u32,
    pub refresh_minutes: u64,
    pub groups: Vec<Group>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub spawnpoints: Vec<Spawn>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spawn {
    pub object_id: i64,
    pub tile: [i32; 2],
    #[serde(default)]
    pub name: String,
}
impl MapConfig {
    pub fn parse(bytes: &[u8]) -> Result<Self, &'static str> {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| "map cfg must be UTF-8")?
            .trim_start_matches('\u{feff}');
        let cfg: Self = serde_json::from_str(text).map_err(|_| "invalid map weather cfg")?;
        if cfg.version != 1
            || !(15..=1440).contains(&cfg.refresh_minutes)
            || cfg.groups.is_empty()
            || cfg.groups.len() > 1000
        {
            return Err("invalid weather cfg version, interval or groups");
        }
        let mut used = HashSet::new();
        for g in &cfg.groups {
            if g.name.trim().is_empty()
                || !g.latitude.is_finite()
                || !g.longitude.is_finite()
                || g.latitude.abs() > 90.0
                || g.longitude.abs() > 180.0
                || g.spawnpoints.is_empty()
            {
                return Err("invalid group name, coordinates or spawnpoints");
            }
            for s in &g.spawnpoints {
                if !used.insert((s.tile, s.object_id)) {
                    return Err("a spawnpoint belongs to more than one group");
                }
            }
        }
        Ok(cfg)
    }
}
pub struct Region {
    pub name: String,
    pub coord: String,
    pub points: Vec<[f64; 2]>,
}
pub fn coord(lat: f64, lon: f64) -> String {
    format!(
        "{:.6},{:.6}",
        if lat == 0.0 { 0.0 } else { lat },
        if lon == 0.0 { 0.0 } else { lon }
    )
}
pub fn nearest(regions: &[Region], at: [f64; 2], current: Option<usize>) -> Option<usize> {
    let distance = |r: &Region| {
        r.points
            .iter()
            .map(|p| ((p[0] - at[0]).powi(2) + (p[1] - at[1]).powi(2)).sqrt())
            .fold(f64::INFINITY, f64::min)
    };
    let best = regions
        .iter()
        .enumerate()
        .filter(|(_, r)| !r.points.is_empty())
        .min_by(|(_, a), (_, b)| distance(a).total_cmp(&distance(b)))
        .map(|(i, _)| i)?;
    Some(
        current
            .filter(|i| {
                *i < regions.len() && distance(&regions[*i]) <= distance(&regions[best]) + 200.0
            })
            .unwrap_or(best),
    )
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Cached {
    pub at: u64,
    values: Value,
}
#[derive(Default, Serialize, Deserialize)]
struct Cache {
    requests: Vec<u64>,
    blocked_until: u64,
    values: HashMap<String, Cached>,
}
impl Cache {
    fn reserve(&mut self, now: u64) -> Result<(), &'static str> {
        self.requests.retain(|t| now.saturating_sub(*t) < 86400);
        if now < self.blocked_until
            || self.requests.len() >= 450
            || self
                .requests
                .iter()
                .filter(|t| now.saturating_sub(**t) < 3600)
                .count()
                >= 20
            || self
                .requests
                .last()
                .is_some_and(|t| now.saturating_sub(*t) < 180)
        {
            return Err("local API budget or retry delay reached; keeping current weather");
        }
        self.requests.push(now);
        Ok(())
    }
}
struct Lock(PathBuf);
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn cache_path(key: &str, settings_dir: Option<&Path>) -> Option<PathBuf> {
    let base = omsi_cfg::flags::OMSI_TOMORROW_CACHE_DIR
        .live_os()
        .map(PathBuf::from)
        .or_else(|| settings_dir.map(|d| d.join("weather-cache")))?;
    Some(cache_path_in(&base, key))
}
fn cache_path_in(base: &Path, key: &str) -> PathBuf {
    use sha2::{Digest, Sha256};
    let hash: String = Sha256::digest(key.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    base.join(format!("tomorrow-{}.json", &hash[..16]))
}
pub fn credential(settings_dir: Option<&Path>) -> Option<String> {
    let path = omsi_cfg::flags::OMSI_TOMORROW_KEY_FILE
        .os()
        .map(PathBuf::from)
        .or_else(|| settings_dir.map(|d| d.join("tomorrow-api-key.txt")));
    read_credential(
        omsi_cfg::flags::OMSI_TOMORROW_API_KEY.var(),
        path.as_deref(),
    )
}
fn read_credential(explicit: Option<&str>, path: Option<&Path>) -> Option<String> {
    let key = explicit
        .map(str::to_owned)
        .or_else(|| std::fs::read_to_string(path?).ok())?;
    let key = key.trim().to_string();
    (!key.is_empty()
        && key.len() < 512
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'))
    .then_some(key)
}
fn write_cache(path: &PathBuf, cache: &Cache) -> Result<(), &'static str> {
    let temp = path.with_extension("tmp");
    std::fs::write(
        &temp,
        serde_json::to_vec(cache).map_err(|_| "cannot encode weather cache")?,
    )
    .map_err(|_| "cannot save weather cache")?;
    std::fs::rename(&temp, path).map_err(|_| "cannot replace weather cache")
}
pub fn fetch<T>(
    key: String,
    coord: &str,
    ttl: u64,
    settings_dir: Option<&Path>,
    parse: fn(&Value) -> Result<T, &'static str>,
) -> Result<Loaded<T>, &'static str> {
    let path = cache_path(&key, settings_dir).ok_or("no private cache directory")?;
    fetch_in(&path, coord, ttl, now(), parse, || request(&key, coord))
}
fn fetch_in<T>(
    path: &PathBuf,
    coord: &str,
    ttl: u64,
    time: u64,
    parse: fn(&Value) -> Result<T, &'static str>,
    request: impl FnOnce() -> Result<ureq::Response, ureq::Error>,
) -> Result<Loaded<T>, &'static str> {
    std::fs::create_dir_all(path.parent().unwrap())
        .map_err(|_| "cannot create private cache directory")?;
    let lock = path.with_extension("lock");
    if std::fs::metadata(&lock)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|t| t.as_secs() > 120)
    {
        let _ = std::fs::remove_file(&lock);
    }
    let _file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock)
        .map_err(|_| "another weather request is running")?;
    let _lock = Lock(lock);
    let mut cache: Cache = match std::fs::read(&path) {
        Ok(b) => {
            serde_json::from_slice(&b).map_err(|_| "weather cache is invalid; no request sent")?
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Cache::default(),
        Err(_) => return Err("cannot read weather cache"),
    };
    if let Some(c) = cache
        .values
        .get(coord)
        .filter(|c| time >= c.at && time - c.at < ttl)
    {
        return Loaded::parse(c.clone(), parse);
    }
    cache.reserve(time)?;
    // Persist the reservation before networking; failed calls count too.
    write_cache(&path, &cache)?;
    let response = match request() {
        Ok(r) => r,
        Err(ureq::Error::Status(status, _)) => {
            if matches!(status, 401 | 403 | 429) {
                cache.blocked_until = time + 3600;
                write_cache(&path, &cache)?;
            }
            return Err(match status {
                429 => "Tomorrow.io quota reached; requests paused for an hour",
                401 | 403 => "Tomorrow.io rejected the private API key",
                _ => "Tomorrow.io returned an HTTP error",
            });
        }
        Err(_) => return Err("Tomorrow.io could not be reached"),
    };
    let mut body = String::new();
    response
        .into_reader()
        .take(128 * 1024)
        .read_to_string(&mut body)
        .map_err(|_| "cannot read Tomorrow.io response")?;
    let data: Value = serde_json::from_str(&body).map_err(|_| "invalid Tomorrow.io JSON")?;
    let values = data
        .get("data")
        .and_then(|d| d.get("values"))
        .filter(|v| v.is_object())
        .ok_or("Tomorrow.io response has no weather values")?
        .clone();
    let observed = parse(&values)?;
    let cached = Cached { at: time, values };
    cache.values.insert(coord.into(), cached.clone());
    write_cache(&path, &cache)?;
    Ok(Loaded { cached, observed })
}
fn request(key: &str, coord: &str) -> Result<ureq::Response, ureq::Error> {
    ureq::AgentBuilder::new()
        .redirects(0)
        .timeout(Duration::from_secs(8))
        .build()
        .get("https://api.tomorrow.io/v4/weather/realtime")
        .query("location", coord)
        .query("units", "metric")
        .set("apikey", key)
        .call()
}
// Parsing stays on the worker, including a cache hit. The provider adapter
// converts API units only; the weather model chooses the game representation.
pub struct Loaded<T> {
    pub cached: Cached,
    pub observed: T,
}
impl<T> Loaded<T> {
    fn parse(
        cached: Cached,
        parse: fn(&Value) -> Result<T, &'static str>,
    ) -> Result<Self, &'static str> {
        let observed = parse(&cached.values)?;
        Ok(Self { cached, observed })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_sites_and_boundary_hysteresis() {
        assert_eq!(coord(50.0, 14.0), coord(50.00000001, 14.00000001));
        let regions = vec![
            Region {
                name: "A".into(),
                coord: "a".into(),
                points: vec![[0.0, 0.0], [100.0, 0.0]],
            },
            Region {
                name: "B".into(),
                coord: "b".into(),
                points: vec![[1000.0, 0.0]],
            },
        ];
        assert_eq!(nearest(&regions, [600.0, 0.0], Some(0)), Some(0));
        assert_eq!(nearest(&regions, [900.0, 0.0], Some(0)), Some(1));
    }
    #[test]
    fn budgets_count_failures_and_survive_serialization() {
        let mut c = Cache::default();
        c.reserve(100000).unwrap();
        assert!(c.reserve(100001).is_err());
        c.requests = (0..20).map(|i| 100000 + i * 180).collect();
        assert!(c.reserve(103599).is_err());
        let text = serde_json::to_vec(&c).unwrap();
        let mut c: Cache = serde_json::from_slice(&text).unwrap();
        c.blocked_until = 200000;
        assert!(c.reserve(104000).is_err());
        c.blocked_until = 0;
        c.requests = (0..450).map(|i| 100000 + i * 180).collect();
        assert!(c.reserve(181000).is_err());
    }
    #[test]
    fn cfg_rejects_credentials_duplicates_and_invalid_coordinates() {
        let s = r#"{"version":1,"refresh_minutes":15,"groups":[{"name":"A","latitude":50,"longitude":14,"spawnpoints":[{"object_id":7,"tile":[0,0]}]}]}"#;
        assert!(MapConfig::parse(s.as_bytes()).is_ok());
        assert!(
            MapConfig::parse(s.replace("\"latitude\":50", "\"latitude\":95").as_bytes()).is_err()
        );
        assert!(
            MapConfig::parse(
                s.replace("\"version\":1", "\"version\":1,\"apikey\":\"never here\"")
                    .as_bytes()
            )
            .is_err()
        );
        assert!(
            MapConfig::parse(
                s.replace(
                    "{\"object_id\":7,\"tile\":[0,0]}",
                    "{\"object_id\":7,\"tile\":[0,0]},{\"object_id\":7,\"tile\":[0,0]}"
                )
                .as_bytes()
            )
            .is_err()
        );
    }
}
