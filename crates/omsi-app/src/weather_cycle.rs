//! The weather cycle (`weather = cycle`: the launcher's "Weather cycle", a server's
//! `weather = cycle`): the weather goes on to another of the installed ones every 25 to
//! 60 minutes of the day, one that suits the month and lies near the one before (clear to
//! cloudy to rain, not sun to snowstorm), and every change of weather - the cycle's, an
//! admin's, the host's in LAN play - comes in over a few minutes instead of at a stroke.
//! OMSI 2 itself keeps one weather for the whole session (its `.owt` files are fixed
//! states; only the real weather of METAR changes, when it is downloaded again).

use omsi_content::weather::Weather;

/// What the weather choice says for the cycle.
pub const CYCLE: &str = "cycle";

pub fn is_cycle(choice: Option<&str>) -> bool {
    choice.is_some_and(|w| w.trim().eq_ignore_ascii_case(CYCLE))
}

/// A change of weather under way: from what it was to what it becomes, `k` of the way.
pub struct Blend {
    from: Weather,
    to: Weather,
    k: f32,
    /// Seconds of the day it takes.
    secs: f32,
    /// The cloud type was switched (halfway).
    switched: bool,
}

impl Blend {
    pub fn new(from: Weather, to: Weather, secs: f32) -> Blend {
        Blend { from, to, k: 0.0, secs: secs.max(1.0), switched: false }
    }

    /// Advance by `dt` seconds of the day: the weather as it is now, whether the cloud type
    /// has just changed (the sky's cloud texture is made again), and whether it is done.
    pub fn step(&mut self, dt: f32) -> (Weather, bool, bool) {
        self.k = (self.k + dt / self.secs).min(1.0);
        let w = lerp(&self.from, &self.to, self.k);
        let switch = !self.switched && self.k >= 0.5;
        if switch {
            self.switched = true;
        }
        let changed_type = switch && self.from.clouds.0.trim() != self.to.clouds.0.trim();
        (w, changed_type, self.k >= 1.0)
    }
}

/// The weather `k` of the way from `a` to `b`: the numbers in between, the kind of clouds
/// and the snow as `b` has them from halfway.
pub fn lerp(a: &Weather, b: &Weather, k: f32) -> Weather {
    let k = k.clamp(0.0, 1.0);
    let m = |x: f32, y: f32| x + (y - x) * k;
    let late = k >= 0.5;
    let n = a.precip.len().max(b.precip.len());
    let p = |v: &Vec<f32>, i: usize| v.get(i).copied().unwrap_or(0.0);
    Weather {
        path: b.path.clone(),
        name: if late { b.name.clone() } else { a.name.clone() },
        description: b.description.clone(),
        // (visibility changes on a log scale: 50 km to 200 m passes through the kilometres)
        fog: ((a.fog.0.max(1.0).ln() + (b.fog.0.max(1.0).ln() - a.fog.0.max(1.0).ln()) * k).exp(), m(a.fog.1, b.fog.1)),
        wind: (m(a.wind.0, b.wind.0), m(a.wind.1, b.wind.1)),
        temp: (m(a.temp.0, b.temp.0), m(a.temp.1, b.temp.1)),
        pressure: m(a.pressure, b.pressure),
        clouds: (if late { b.clouds.0.clone() } else { a.clouds.0.clone() }, m(a.clouds.1, b.clouds.1)),
        precip: (0..n).map(|i| m(p(&a.precip, i), p(&b.precip, i))).collect(),
        ground_wet: [m(a.ground_wet[0], b.ground_wet[0]), m(a.ground_wet[1], b.ground_wet[1]), m(a.ground_wet[2], b.ground_wet[2])],
        snow: if late { b.snow } else { a.snow },
        snow_on_road: if late { b.snow_on_road } else { a.snow_on_road },
    }
}

/// The installed weathers (`Weather/*.owt`), as (file, weather).
pub fn installed() -> Vec<(String, Weather)> {
    let mut files: Vec<String> = omsi_cfg::read_dir_merged("Weather")
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("owt")))
        .filter_map(|p| p.file_name().map(|n| format!("Weather/{}", n.to_string_lossy())))
        .collect();
    files.sort();
    files.dedup();
    files
        .into_iter()
        .filter_map(|f| {
            let path = omsi_cfg::find_in_roots(&f).map(|x| x.1)?;
            Weather::load(&path).ok().map(|w| (f, w))
        })
        .collect()
}

/// How much of the sky its clouds cover (0 none .. 1 overcast), from the cloud type of its
/// `[clouds]` (-1 none, Cumulus 1..3, Overcast): the number beside it is their height.
fn cover(w: &Weather) -> f32 {
    let t = w.clouds.0.trim().to_ascii_lowercase();
    if t.starts_with("overcast") {
        1.0
    } else if let Some(n) = t.strip_prefix("cumulus") {
        0.15 + 0.2 * n.trim().parse::<f32>().unwrap_or(1.0).clamp(1.0, 3.0)
    } else {
        0.0
    }
}

/// How much rain or snow a weather brings (0 none .. 1 a downpour).
fn wetness(w: &Weather) -> f32 {
    w.precip.first().copied().unwrap_or(0.0).clamp(0.0, 1.0).max(w.precip.get(1).copied().unwrap_or(0.0) / 32.0).clamp(0.0, 1.0)
}

/// Does the weather suit month `m` (1..12)? Snow only from November to March, no frost in
/// summer, no heat in winter.
fn suits(w: &Weather, m: i32) -> bool {
    let winter = matches!(m, 11 | 12 | 1 | 2 | 3);
    let summer = matches!(m, 5..=9);
    if w.snow && !winter {
        return false;
    }
    if summer && w.temp.0 < 3.0 {
        return false;
    }
    if winter && w.temp.0 > 16.0 {
        return false;
    }
    true
}

/// The next weather after `now` (a file of `all`): one that suits the month, the nearer
/// the likelier (in cloud cover, rain and visibility). `r` is a random number 0..1.
pub fn pick(all: &[(String, Weather)], now: &Weather, now_file: &str, month: i32, r: f32) -> Option<String> {
    let cand: Vec<&(String, Weather)> = all.iter().filter(|(f, w)| !f.eq_ignore_ascii_case(now_file) && suits(w, month)).collect();
    let cand = if cand.is_empty() { all.iter().filter(|(f, _)| !f.eq_ignore_ascii_case(now_file)).collect() } else { cand };
    if cand.is_empty() {
        return None;
    }
    let vis = |w: &Weather| (w.fog.0.max(100.0).ln() - 100f32.ln()) / (60000f32.ln() - 100f32.ln());
    let weights: Vec<f32> = cand
        .iter()
        .map(|(_, w)| {
            let d = (cover(w) - cover(now)).abs() * 1.5 + (wetness(w) - wetness(now)).abs() * 3.0 + (vis(w) - vis(now)).abs() * 1.5;
            1.0 / (0.25 + d * d)
        })
        .collect();
    let total: f32 = weights.iter().sum();
    let mut x = r.clamp(0.0, 0.9999) * total;
    for (c, w) in cand.iter().zip(&weights) {
        if x < *w {
            return Some(c.0.clone());
        }
        x -= w;
    }
    cand.last().map(|c| c.0.clone())
}

/// The cycle: when the weather changes next (seconds of the day, counted down) and its dice.
pub struct Cycle {
    pub next_in: f64,
    rng: u64,
}

impl Cycle {
    pub fn new(seed: u64) -> Cycle {
        let mut c = Cycle { next_in: 0.0, rng: seed | 1 };
        c.next_in = c.interval();
        c
    }

    pub fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        (x >> 40) as f32 / (1u64 << 24) as f32
    }

    /// 25 to 60 minutes of the day.
    pub fn interval(&mut self) -> f64 {
        (25.0 + 35.0 * self.rand() as f64) * 60.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(name: &str, cover: f32, rain: f32, fog: f32, temp: f32, snow: bool) -> Weather {
        Weather { name: name.into(), fog: (fog, 0.0), temp: (temp, 0.0), clouds: (if cover > 0.5 { "Overcast 1".into() } else if cover > 0.2 { "Cumulus 2".into() } else { "-1".into() }, 300.0), precip: vec![rain, 0.0, 0.0, 0.0, 0.0], snow, ..Default::default() }
    }

    #[test]
    fn a_blend_passes_through_the_middle_and_switches_the_clouds_halfway() {
        let (a, b) = (w("clear", 0.1, 0.0, 50000.0, 20.0, false), w("rain", 0.9, 1.0, 2000.0, 12.0, false));
        let mut bl = Blend::new(a, b, 100.0);
        let (x, changed, done) = bl.step(25.0);
        assert!(!changed && !done && (x.temp.0 - 18.0).abs() < 1e-3 && x.clouds.0 == "-1");
        let (x, changed, _) = bl.step(30.0);
        assert!(changed && x.clouds.0 == "Overcast 1" && x.fog.0 < 50000.0 && x.fog.0 > 2000.0);
        let (x, _, done) = bl.step(60.0);
        assert!(done && x.name == "rain" && (x.precip[0] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn the_next_weather_suits_the_month_and_is_near() {
        let all = vec![
            ("Weather/a.owt".to_string(), w("clear", 0.1, 0.0, 50000.0, 20.0, false)),
            ("Weather/b.owt".to_string(), w("cloudy", 0.4, 0.0, 30000.0, 18.0, false)),
            ("Weather/c.owt".to_string(), w("snow", 1.0, 1.0, 800.0, -5.0, true)),
        ];
        // in July there is no snow; from clear, cloudy is the only near one left
        for r in [0.0, 0.3, 0.7, 0.99] {
            assert_eq!(pick(&all, &all[0].1, "Weather/a.owt", 7, r).as_deref(), Some("Weather/b.owt"));
        }
        // in January snow may come
        assert!((0..20).map(|i| pick(&all, &all[1].1, "Weather/b.owt", 1, i as f32 / 20.0)).any(|p| p.as_deref() == Some("Weather/c.owt")));
    }
    #[test]
    fn the_clouds_drift_smoothly_while_the_wind_blends() {
        let mut a = w("calm", 0.4, 0.0, 50000.0, 20.0, false);
        a.wind = (0.0, 1.0);
        let mut b = w("storm", 0.4, 0.0, 50000.0, 20.0, false);
        b.wind = (90.0, 20.0);
        let mut bl = Blend::new(a, b, 240.0);
        // late in the year, where the old time-based drift jumped by whole tiles a frame
        let mut d = crate::weather_setup::cloud_drift_at(&bl.from, 3.0e7);
        for _ in 0..600 {
            let (x, _, _) = bl.step(1.0 / 2.0);
            let before = d;
            crate::weather_setup::cloud_drift_step(&mut d, &x, 1.0 / 2.0);
            for i in 0..2 {
                let step = (d[i] - before[i]).abs();
                assert!(step.min(1.0 - step) < 0.01, "jump {step}");
            }
        }
    }
}
