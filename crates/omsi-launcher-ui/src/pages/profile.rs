//! The Profile page: the driver's personnel file.

use super::*;
use omsi_launcher_lib as core;

// --- profile --------------------------------------------------------------------------------

pub fn profile(l: &mut Launcher, area: Rect) {
    let body = l.page_title(area, "Profile", "The driver whose personnel file the game writes: hours, kilometres, punctuality, tickets.");
    let left_w = (body.w * 0.56).min(700.0);
    let left = Rect::new(body.x, body.y, left_w, body.h);
    let right = Rect::new(body.x + left_w + GAP * 2.0, body.y, body.w - left_w - GAP * 2.0, body.h);
    // driver chooser
    let top = Rect::new(left.x, left.y, left.w, 132.0);
    l.ui.panel(top);
    let inner = l.ui.heading(Rect::new(top.x + 18.0, top.y + 14.0, top.w - 36.0, top.h - 28.0), "Driver", Some("person"));
    let names = l.state.profiles.clone();
    let mut sel = names.iter().position(|n| *n == l.state.config.profile).unwrap_or(0);
    let half = (inner.w - GAP) * 0.5;
    if !names.is_empty() && l.ui.select("profile", Rect::new(inner.x, inner.y, half, ROW), &mut sel, &names) {
        l.state.config.profile = names[sel].clone();
        let _ = core::save_config(&l.state.config);
        l.state.load_profile();
        l.state.touched();
    }
    let danger_armed = l.pages.confirm_delete.map(|t| t.elapsed().as_secs() < 4).unwrap_or(false);
    if l.ui.button("profile-delete", Rect::new(inner.x + half + GAP, inner.y, half, ROW), if danger_armed { "Click again to delete" } else { "Delete this driver" }, Some("delete"), ButtonKind::Danger) {
        if danger_armed {
            let name = l.state.config.profile.clone();
            match core::delete_profile(&name) {
                Ok(()) => {
                    l.state.profile = None;
                    l.state.config.profile.clear();
                    let _ = core::save_config(&l.state.config);
                    l.state.set_status(format!("Personnel file of {name} deleted."), false);
                    l.state.load_profiles();
                }
                Err(e) => l.state.set_status(format!("{e:#}"), true),
            }
            l.pages.confirm_delete = None;
        } else {
            l.pages.confirm_delete = Some(std::time::Instant::now());
        }
    }
    let y = inner.y + ROW + 10.0;
    l.ui.text_input("new-driver", Rect::new(inner.x, y, half, ROW), &mut l.pages.new_driver, "New driver's name", Some("person"));
    if l.ui.button("profile-create", Rect::new(inner.x + half + GAP, y, half, ROW), "Create", Some("add"), ButtonKind::Normal) {
        let name = l.pages.new_driver.trim().to_string();
        if !name.is_empty() {
            match core::create_profile(&name, "M") {
                Ok(_) => {
                    l.state.config.profile = name.clone();
                    let _ = core::save_config(&l.state.config);
                    l.pages.new_driver.clear();
                    l.state.load_profiles();
                    l.state.set_status(format!("Driver {name} created."), false);
                }
                Err(e) => l.state.set_status(format!("{e:#}"), true),
            }
        }
    }
    // level and stats
    let card = Rect::new(left.x, top.bottom() + GAP * 1.5, left.w, left.h - top.h - GAP * 1.5);
    l.ui.panel(card);
    let Some(p) = l.state.profile.clone() else {
        l.ui.text_in("Create a driver to start a personnel file.", card.pad(20.0, 20.0), 14.0, Weight::Medium, TEXT_DIM, Align::Left);
        return;
    };
    let c = Vec2::new(card.x + 70.0, card.y + 76.0);
    let prev = ((p.level - 1) * (p.level - 1) * 250) as f64;
    let frac = ((p.xp as f64 - prev) / (p.next_level_xp as f64 - prev).max(1.0)).clamp(0.0, 1.0) as f32;
    let shown = l.ui.anim(id_of("xp-ring"), frac, 0.6);
    l.ui.p().circle(c, 50.0, Color::rgba(28, 31, 37, 1.0));
    l.ui.p().arc(c, 44.0, 52.0, 0.0, std::f32::consts::TAU, Color::WHITE.alpha(0.08));
    let a0 = -std::f32::consts::FRAC_PI_2;
    l.ui.p().arc(c, 44.0, 52.0, a0, a0 + std::f32::consts::TAU * shown.max(0.01), ACCENT);
    l.ui.text_in(&p.level.to_string(), Rect::new(c.x - 40.0, c.y - 26.0, 80.0, 40.0), 34.0, Weight::Black, TEXT, Align::Center);
    l.ui.text_in("LEVEL", Rect::new(c.x - 40.0, c.y + 12.0, 80.0, 16.0), 10.0, Weight::Black, TEXT_DIM, Align::Center);
    let tx = card.x + 142.0;
    l.ui.text_in(&format!("{}{}", p.name, if p.exists { "" } else { " (no personnel file yet)" }), Rect::new(tx, card.y + 34.0, card.w - 160.0, 30.0), 24.0, Weight::Black, TEXT, Align::Left);
    l.ui.progress(Rect::new(tx, card.y + 76.0, card.w - 170.0, 10.0), shown, false);
    l.ui.text_in(&format!("{} XP | {} to level {}", p.xp, (p.next_level_xp - p.xp).max(0), p.level + 1), Rect::new(tx, card.y + 94.0, card.w - 160.0, 18.0), 12.5, Weight::Medium, TEXT_DIM, Align::Left);
    let hours = |h: f64| format!("{} h {:02} min", h.floor() as i64, ((h - h.floor()) * 60.0).round() as i64);
    let stats = [
        ("schedule", hours(p.hours), "hours driven"),
        ("route", format!("{:.1} km", p.km), "distance"),
        ("location_on", p.stops.to_string(), "stops served"),
        ("timer", format!("{} / {}", p.early, p.late), "early / late"),
        ("confirmation_number", format!("{:.0}", p.tickets), "tickets sold"),
        ("payments", format!("{:.2}", p.cash), "takings"),
        ("warning", p.crashes.to_string(), "crashes"),
        ("person", p.hurt.to_string(), "pedestrians hurt"),
        ("speed", format!("{:.0} %", p.rating_driving), "driving"),
        ("airport_shuttle", format!("{:.0} %", p.rating_comfort), "comfort"),
        ("receipt_long", format!("{:.0} %", p.rating_tickets), "ticket selling"),
        ("history", p.sessions.len().to_string(), "runs"),
    ];
    let grid = Rect::new(card.x + 18.0, card.y + 150.0, card.w - 36.0, card.h - 170.0);
    let cols = 3;
    let cw = (grid.w - GAP * (cols as f32 - 1.0)) / cols as f32;
    let ch = 68.0;
    for (k, (icon, v, label)) in stats.iter().enumerate() {
        let (cx, cy) = ((k % cols) as f32, (k / cols) as f32);
        let r = Rect::new(grid.x + cx * (cw + GAP), grid.y + cy * (ch + 10.0), cw, ch);
        if r.bottom() > card.bottom() - 6.0 {
            break;
        }
        l.ui.p().rounded(r, 10.0, Color::WHITE.alpha(0.04));
        l.ui.icon(icon, Vec2::new(r.x + 22.0, r.y + 22.0), 18.0, ACCENT);
        l.ui.text_in(v, Rect::new(r.x + 40.0, r.y + 8.0, r.w - 48.0, 28.0), 18.0, Weight::Black, TEXT, Align::Left);
        l.ui.text_in(label, Rect::new(r.x + 14.0, r.y + 42.0, r.w - 20.0, 18.0), 11.5, Weight::Medium, TEXT_DIM, Align::Left);
    }
    // recent runs
    l.ui.panel(right);
    let inner = l.ui.heading(Rect::new(right.x + 18.0, right.y + 14.0, right.w - 36.0, right.h - 28.0), "Recent runs", Some("history"));
    let sessions = p.sessions.clone();
    l.ui.scroll_area("runs", Rect::new(inner.x - 6.0, inner.y, inner.w + 12.0, inner.h), &mut |ui, v| {
        if sessions.is_empty() {
            ui.text_in("No runs yet. Drive a duty and it shows up here.", Rect::new(v.x + 8.0, v.y, v.w, 30.0), 13.0, Weight::Regular, TEXT_DIM, Align::Left);
            return 30.0;
        }
        let rh = 62.0;
        for (k, s) in sessions.iter().enumerate() {
            let r = Rect::new(v.x + 6.0, v.y + k as f32 * rh, v.w - 16.0, rh - 6.0);
            ui.p().rounded(r, 9.0, Color::WHITE.alpha(0.04));
            let title = match &s.line {
                Some(line) => format!("Line {line}{} | {}", s.tour.as_ref().map(|t| format!(" / {t}")).unwrap_or_default(), short_map(&s.map)),
                None => format!("Free drive | {}", short_map(&s.map)),
            };
            ui.text_in(&title, Rect::new(r.x + 12.0, r.y + 6.0, r.w - 130.0, 20.0), 13.0, Weight::Bold, TEXT, Align::Left);
            ui.text_in(&format!("{} | {:.1} km | {} stops | {} tickets | {} crashes", s.bus.rsplit('/').next().unwrap_or(""), s.metres / 1000.0, s.stops, s.tickets, s.crashes), Rect::new(r.x + 12.0, r.y + 28.0, r.w - 130.0, 18.0), 11.0, Weight::Regular, TEXT_DIM, Align::Left);
            let when = chrono_like(s.time);
            ui.text_in(&when, Rect::new(r.right() - 120.0, r.y + 6.0, 110.0, 20.0), 11.5, Weight::Medium, TEXT_SOFT, Align::Right);
            ui.text_in(&hours_short(s.seconds / 3600.0), Rect::new(r.right() - 120.0, r.y + 28.0, 110.0, 18.0), 11.5, Weight::Medium, ACCENT, Align::Right);
        }
        sessions.len() as f32 * rh
    });
}

fn hours_short(h: f64) -> String {
    format!("{}:{:02} h", h.floor() as i64, ((h - h.floor()) * 60.0).round() as i64)
}

/// A Unix time as "YYYY-MM-DD HH:MM" in the machine's time zone.
pub(super) fn chrono_like(t: u64) -> String {
    #[cfg(unix)]
    {
        let tt = t as libc::time_t;
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        if !unsafe { libc::localtime_r(&tt, &mut tm) }.is_null() {
            return format!("{:04}-{:02}-{:02} {:02}:{:02}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min);
        }
    }
    let days = (t / 86400) as i64;
    let secs = t % 86400;
    // civil from days (Howard Hinnant), UTC
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02} UTC", secs / 3600, (secs % 3600) / 60)
}
