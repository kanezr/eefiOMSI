//! Host and client clock alignment across dates and midnight.
use omsi_net::LanSession;

/// A date `YYYY-MM-DD` as (year, day of year).
pub fn parse_date(s: &str) -> Option<(i32, i32)> {
    let v: Vec<i32> = s.split('-').filter_map(|x| x.trim().parse().ok()).collect();
    (v.len() == 3).then(|| {
        let mut c = omsi_sim::SimClock::default();
        c.set_date(v[0], v[1], v[2]);
        (c.year, c.day_of_year)
    })
}

/// The host's clock as a `SimClock` now (its date moved on past midnight).
pub fn host_clock_now(h: &omsi_net::HostClock) -> Option<omsi_sim::SimClock> {
    let (year, day_of_year) = parse_date(&h.world.date)?;
    let mut c = omsi_sim::SimClock {
        year,
        day_of_year,
        time: h.world.time,
        ..Default::default()
    };
    c.advance(h.at.elapsed().as_secs_f32() * h.speed as f32);
    Some(c)
}

/// The host's time of day now as `HH:MM:SS` (a joining player's duty is placed with it).
pub fn host_time_now(lan: &LanSession) -> Option<String> {
    let w = lan.welcome.as_ref()?;
    let c = host_clock_now(&omsi_net::HostClock {
        world: w.world.clone(),
        at: w.at,
        speed: 1.0,
    })?;
    let t = c.time.rem_euclid(86400.0) as u32;
    Some(format!("{:02}:{:02}:{:02}", t / 3600, t / 60 % 60, t % 60))
}

pub fn day_number(year: i32, day_of_year: i32) -> i64 {
    let y = year as i64 - 1;
    let before = if year > 0 {
        365 * y + y / 4 - y / 100 + y / 400 + 366
    } else {
        0
    };
    before + day_of_year as i64
}

/// How far our clock is behind the host's (s, negative: ahead).
pub fn clock_gap(host: &omsi_sim::SimClock, mine: &omsi_sim::SimClock) -> f64 {
    (day_number(host.year, host.day_of_year) - day_number(mine.year, mine.day_of_year)) as f64
        * 86400.0
        + host.time
        - mine.time
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    #[test]
    fn day_numbers_and_clock_gaps() {
        assert_eq!(day_number(1990, 1) - day_number(1989, 365), 1);
        assert_eq!(day_number(1989, 1) - day_number(1988, 366), 1);
        let a = omsi_sim::SimClock {
            year: 1990,
            day_of_year: 1,
            time: 10.0,
            ..Default::default()
        };
        let b = omsi_sim::SimClock {
            year: 1989,
            day_of_year: 365,
            time: 86390.0,
            ..Default::default()
        };
        assert!((clock_gap(&a, &b) - 20.0).abs() < 1e-9);
        assert!((clock_gap(&b, &a) + 20.0).abs() < 1e-9);
        assert_eq!(parse_date("1989-05-30"), Some((1989, 150)));
        assert_eq!(parse_date("x"), None);
    }

    #[test]
    fn the_hosts_clock_moves_on_past_midnight() {
        let h = omsi_net::HostClock {
            world: omsi_net::WorldInfo {
                date: "1989-12-31".into(),
                time: 86399.5,
                ..Default::default()
            },
            at: Instant::now() - Duration::from_secs(2),
            speed: 1.0,
        };
        let c = host_clock_now(&h).unwrap();
        assert_eq!((c.year, c.day_of_year), (1990, 1));
        assert!((c.time - 1.5).abs() < 0.1, "{}", c.time);
    }
}
