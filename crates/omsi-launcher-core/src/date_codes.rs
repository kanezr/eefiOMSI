/// A date code (YYYYMMDD) `k` days later.
pub(crate) fn add_days(code: i32, k: i32) -> i32 {
    let (mut y, mut m, mut d) = (code / 10000, code / 100 % 100, code % 100 + k);
    let len = |y: i32, m: i32| match m {
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    while d > len(y, m) {
        d -= len(y, m);
        m += 1;
        if m > 12 {
            m = 1;
            y += 1;
        }
    }
    y * 10000 + m * 100 + d
}

/// The date code (YYYYMMDD) of the day before.
pub(crate) fn day_before(code: i32) -> i32 {
    let (y, m, d) = (code / 10000, code / 100 % 100, code % 100);
    if d > 1 {
        return code - 1;
    }
    let (y, m) = if m > 1 { (y, m - 1) } else { (y - 1, 12) };
    let last = match m {
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    y * 10000 + m * 100 + last
}

/// Day of the week of a date code (YYYYMMDD): 0 = Monday … 6 = Sunday, as the game's clock.
pub(crate) fn weekday(code: i32) -> i32 {
    let (mut y, m, d) = (code / 10000, code / 100 % 100, code % 100);
    // Sakamoto's method (0 = Sunday)
    let t = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    if m < 3 {
        y -= 1;
    }
    let sunday0 = (y + y / 4 - y / 100 + y / 400 + t[((m - 1).clamp(0, 11)) as usize] + d).rem_euclid(7);
    (sunday0 + 6) % 7
}

/// A tour's validity mask in words: "Mon-Fri", "Sat", "Sun", "daily", ...
pub(crate) fn days_of(mask: i32) -> String {
    let week = mask & 0x7f;
    let names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let mut out = match week {
        0x7f => "daily".to_string(),
        0x1f => "Mon-Fri".to_string(),
        0x3f => "Mon-Sat".to_string(),
        0x60 => "Sat-Sun".to_string(),
        0 => String::new(),
        w => (0..7).filter(|i| w & (1 << i) != 0).map(|i| names[i]).collect::<Vec<_>>().join(", "),
    };
    if mask & (1 << 7) != 0 && week != 0x7f {
        out.push_str(if out.is_empty() { "holidays" } else { " & holidays" });
    }
    match (mask & (1 << 8) != 0, mask & (1 << 9) != 0) {
        (true, false) => out.push_str(", school days"),
        (false, true) => out.push_str(", school holidays"),
        _ => {}
    }
    out
}
