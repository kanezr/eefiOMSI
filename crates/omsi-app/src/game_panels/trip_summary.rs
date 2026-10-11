//! The trip's summary when it ends: a table of its stops - planned, actual, how far off,
//! the odometer there - with the stops served and the kilometres driven, to copy or to keep
//! in the Journeys folder beside the journey's log. Settings → Gameplay → "Trip summary"
//! turns it off; the quick menu's statistics tile shows the last one again.

use super::{button, card, el, grow_row, note, row, text, title};
use crate::journey::TripSummary;
use omsi_plugin::ui::{Anchor, Kind, Panel, RowAlign, Weight};

pub(crate) struct TripPanel {
    summary: TripSummary,
    /// What the last button did (copied, saved where).
    said: Option<String>,
}

impl TripPanel {
    pub(crate) fn new(summary: TripSummary) -> TripPanel {
        TripPanel { summary, said: None }
    }

    pub(crate) fn panel(&self) -> Panel {
        let s = &self.summary;
        let mut children = vec![title("Trip summary"), text(&s.title, 14.0, Weight::Medium), note(&s.head)];
        let rows: Vec<Vec<String>> = s.rows.iter().map(|r| r.to_vec()).collect();
        children.push(el(
            None,
            Kind::Table {
                columns: ["Stop", "Plan", "Actual", "Diff", "Odometer"].map(String::from).to_vec(),
                rows,
                widths: vec![0.36, 0.15, 0.15, 0.16, 0.18],
                size: 13.0,
            },
        ));
        let mut tally = format!("{} {} / {}", omsi_ui::tr("Stops served:"), s.served.0, s.served.1);
        if let Some(km) = s.km {
            tally.push_str(&format!(",  {km:.1} km"));
        }
        children.push(text(&tally, 13.0, Weight::Medium));
        children.push(grow_row(vec![button("copy", "Copy", Some("content_copy")), button("save", "Save to Journeys", Some("save")), button("close", "Close", Some("close"))]));
        if let Some(said) = &self.said {
            children.push(note(said));
        }
        children.push(row(vec![note("Settings → Gameplay → Trip summary turns it off.")], RowAlign::Start));
        card(Anchor::Center, 0.0, 0.0, 620.0, 236, children)
    }
}

/// The text on the system's clipboard; false when there is none to be had.
pub(crate) fn to_clipboard(text: &str) -> bool {
    #[cfg(not(target_os = "android"))]
    {
        thread_local! {
            // (kept alive: on X11 the text is gone when the clipboard is dropped)
            static CLIPBOARD: std::cell::RefCell<Option<arboard::Clipboard>> = const { std::cell::RefCell::new(None) };
        }
        CLIPBOARD.with(|c| {
            let mut c = c.borrow_mut();
            if c.is_none() {
                *c = arboard::Clipboard::new().ok();
            }
            c.as_mut().is_some_and(|cb| cb.set_text(text.to_string()).is_ok())
        })
    }
    #[cfg(target_os = "android")]
    {
        let _ = text;
        false
    }
}

pub(crate) fn click(app: &mut crate::App, id: &str) {
    let Some(t) = app.menus.game_panels.trip.as_mut() else { return };
    match id {
        "copy" => {
            t.said = Some(if to_clipboard(&t.summary.text()) { omsi_ui::tr("Copied to the clipboard").into_owned() } else { omsi_ui::tr("The clipboard cannot be reached").into_owned() });
        }
        "save" => {
            t.said = Some(match t.summary.save() {
                Ok(p) => format!("{} {}", omsi_ui::tr("Saved:"), p.display()),
                Err(e) => format!("{} {e}", omsi_ui::tr("Not saved:")),
            });
        }
        "close" => app.menus.game_panels.trip = None,
        _ => {}
    }
}
