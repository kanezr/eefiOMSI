//! Simulation time: date, time of day and the per-frame `Timegap`.

#[derive(Debug, Clone)]
pub struct SimClock {
    pub year: i32,
    pub day_of_year: i32,
    /// Seconds since midnight.
    pub time: f64,
    /// Last frame duration in seconds.
    pub timegap: f32,
    pub paused: bool,
    /// Seconds of play since the clock started - the scripts' `GetTime` (OMSI keeps a
    /// millisecond counter of frame times, `g_859ea8`, and never wraps it at midnight: the
    /// gearbox scripts compare timestamps of it).
    pub run_time: f64,
}

impl Default for SimClock {
    fn default() -> Self {
        Self { year: 1989, day_of_year: 150, time: 9.0 * 3600.0, timegap: 1.0 / 60.0, paused: false, run_time: 0.0 }
    }
}

impl SimClock {
    pub fn advance(&mut self, dt: f32) {
        self.timegap = dt;
        if !self.paused {
            self.run_time += dt as f64;
            self.time += dt as f64;
            while self.time >= 86400.0 {
                self.time -= 86400.0;
                self.day_of_year += 1;
                if self.day_of_year > days_in_year(self.year) {
                    self.day_of_year = 1;
                    self.year += 1;
                }
            }
        }
    }

    pub fn hour(&self) -> f32 {
        (self.time / 3600.0) as f32
    }

    /// (day, month) of the current day of year.
    pub fn day_month(&self) -> (i32, i32) {
        let leap = days_in_year(self.year) == 366;
        let months = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        let mut d = self.day_of_year.max(1);
        for (i, m) in months.iter().enumerate() {
            if d <= *m {
                return (d, i as i32 + 1);
            }
            d -= m;
        }
        (31, 12)
    }

    /// Day of the week, 0 = Monday … 6 = Sunday.
    pub fn weekday(&self) -> i32 {
        // days since 1970-01-01 (a Thursday)
        let mut days: i64 = 0;
        if self.year >= 1970 {
            for y in 1970..self.year {
                days += days_in_year(y) as i64;
            }
        } else {
            for y in self.year..1970 {
                days -= days_in_year(y) as i64;
            }
        }
        days += (self.day_of_year - 1) as i64;
        ((days + 3).rem_euclid(7)) as i32
    }

    /// Set the date from year, month, day.
    pub fn set_date(&mut self, year: i32, month: i32, day: i32) {
        // (a date from a file or another game may be anything: the day is kept within its
        // month and the year within reason, so the sums below cannot overflow)
        let year = year.clamp(1, 9999);
        let leap = days_in_year(year) == 366;
        let months = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        let month = month.clamp(1, 12) as usize;
        let mut doy = day.clamp(1, months[month - 1]);
        for m in months.iter().take(month - 1) {
            doy += m;
        }
        self.year = year;
        self.day_of_year = doy;
    }

    /// `YYYYMMDD` as used by calendars and chrono events.
    pub fn date_code(&self) -> i32 {
        let (d, m) = self.day_month();
        self.year * 10000 + m * 100 + d
    }
}

pub fn days_in_year(y: i32) -> i32 {
    if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
        366
    } else {
        365
    }
}

#[cfg(test)]
mod date_tests {
    use super::*;

    #[test]
    fn dates_from_elsewhere_stay_in_range() {
        let mut c = SimClock::default();
        c.set_date(2024, 3, 1);
        assert_eq!((c.year, c.day_of_year), (2024, 61));
        c.set_date(2023, 2, 31);
        assert_eq!(c.day_of_year, 59, "the day kept within its month");
        c.set_date(i32::MAX, i32::MAX, i32::MAX);
        assert!(c.day_of_year <= 366 && c.year <= 9999);
    }
}
