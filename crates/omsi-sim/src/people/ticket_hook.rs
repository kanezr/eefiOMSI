//! The plugins' say in a passenger's ticket: who boards the player's bus is reported
//! ([`TicketAsk`], once, when they have decided what they do about a ticket), and a plugin may
//! change it ([`TicketChange`]) before they ask for it at the cash desk - a tariff of zones,
//! a pass that needs no ticket, a voice line of its own. Without a plugin listening nothing
//! changes: the pack's ticket, its price and `Ticket_<n>_<k>`.

use super::*;

/// The longest a passenger waits - in the doorway, and at the cash desk - for the plugins
/// to have their say (s of game time). The wait normally ends when the plugins have heard
/// of them, in the next frame (`ticket_asks_heard`); this only bounds it for a frame that
/// never comes (the plugins gone), and leaves room for heavy time acceleration.
pub const TICKET_HOLD: f64 = 5.0;

/// The most passengers reported between two plugin frames (the rest are not reported, and
/// do not wait).
const MAX_ASKS: usize = 256;

/// The longest text a plugin may give a passenger (a name, a voice line, its tag).
const MAX_TEXT: usize = 256;

/// A passenger boarding the player's bus, as they decided about their ticket.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TicketAsk {
    /// The person (`Person::id`, the id `people.list` gives).
    pub person: u32,
    pub age: f32,
    /// `TICKET_NONE` (nothing to do), `TICKET_STAMP` (a ticket to validate) or `TICKET_BUY`.
    pub kind: u8,
    /// The ticket of the pack (from 1; 0: none fits their age) and the name and price they
    /// ask for at the desk (for 0 the pack's first ticket's, as in OMSI).
    pub ticket: u8,
    pub name: String,
    pub price: f32,
    /// The stop they board at: its map object id and its name (the timetable's).
    pub stop: Option<i64>,
    pub stop_name: Option<String>,
    /// Where they ride to: the stop's name, as the timetable names it.
    pub destination: Option<String>,
    /// Where the two are in the duty's trip (from 0), where the trip has them.
    pub stop_index: Option<usize>,
    pub destination_index: Option<usize>,
}

/// What a plugin changes of a passenger's ticket; `None` leaves that part as it is.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TicketChange {
    /// `TICKET_NONE`, `TICKET_STAMP` or `TICKET_BUY`: only before they are in the bus.
    pub kind: Option<u8>,
    /// Another ticket of the pack (from 1): its name and price with it. A buyer always has
    /// one (0 is refused for them).
    pub ticket: Option<u8>,
    /// The name and price asked for, instead of the pack's.
    pub name: Option<String>,
    pub price: Option<f32>,
    /// The voice line said at the desk instead of `Ticket_<n>_<k>` (a file `<name>.wav` of
    /// the passenger's voice folder; where it has none, the usual line). Empty: the usual.
    pub voice: Option<String>,
    /// A text of the plugin's own, kept with the passenger (empty: none).
    pub tag: Option<String>,
    /// The price the passenger pays at the desk (the plugin may change it, or leave it as is)
    pub payment: Option<f32>,
}

impl TicketChange {
    fn changes_ticket(&self) -> bool {
        self.kind.is_some() || self.ticket.is_some() || self.name.is_some() || self.price.is_some() || self.voice.is_some()
    }
}

/// What a plugin made of a passenger's ticket (`Pax::plugin`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PluginTicket {
    /// The name asked for instead of the pack's (the price is then `Pax::price`).
    pub name: Option<String>,
    /// The voice line said at the desk instead of `Ticket_<n>_<k>`.
    pub voice: Option<String>,
    /// The price the passenger pays at the desk (the plugin may change it, or leave it as is)
    pub payment: Option<f32>,
    /// The plugin that changed the ticket: its name and `Pax::price` are asked for.
    pub by: Option<String>,
    /// A text of the plugin's own, kept with the passenger (a new ticket keeps it).
    pub tag: Option<String>,
}

/// A passenger nobody changed.
static NO_PLUGIN: PluginTicket = PluginTicket { name: None, voice: None, payment: None, by: None, tag: None };

impl Pax {
    /// What a plugin made of the ticket (nothing for most passengers).
    pub fn plugin(&self) -> &PluginTicket {
        self.plugin.as_deref().unwrap_or(&NO_PLUGIN)
    }

    fn plugin_mut(&mut self) -> &mut PluginTicket {
        self.plugin.get_or_insert_with(Default::default)
    }

    /// Back to no box once nothing is left in it.
    fn tidy_plugin(&mut self) {
        if self.plugin.as_deref().is_some_and(|p| *p == NO_PLUGIN) {
            self.plugin = None;
        }
    }
}

fn text(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.chars().take(MAX_TEXT).collect())
}

impl PeopleSim {
    /// Passenger `i` has decided about a ticket (`kind`, the pack's ticket `id` from 1, its
    /// `price`) on their way to a bus: what a plugin made of an earlier ticket - a boarding
    /// given up at a shut door, a bus that pulled away - is forgotten (not their tag, which
    /// is the plugin's own), and for the player's bus the plugins hear of it.
    pub fn new_ticket(&mut self, i: usize, kind: u8, id: u8, price: f32, player_bus: bool) {
        let Some(p) = self.pax_mut(i) else { return };
        p.ticket = kind;
        p.ticket_id = id;
        p.price = price;
        p.ticket_hold = None;
        if let Some(pl) = p.plugin.as_deref_mut() {
            (pl.name, pl.voice, pl.by, pl.payment) = (None, None, None, None);
        }
        p.tidy_plugin();
        if player_bus {
            self.ask_ticket(i);
        }
    }

    /// Passenger `i` was reported and the plugins have not had their frame yet: they wait
    /// in the doorway and at the cash desk - however many steps of the people come before
    /// that frame - `TICKET_HOLD` at most, and not at all once no plugin listens.
    pub fn ticket_pending(&self, i: usize) -> bool {
        self.ticket_hook && self.pax(i).and_then(|p| p.ticket_hold).is_some_and(|t| self.time >= t && self.time - t < TICKET_HOLD)
    }

    /// The pack's ticket `id` (from 1) as the desk asks for it: its name and value; for 0
    /// the first ticket's, as Omsi.exe asked.
    pub fn pack_ticket(&self, id: u8) -> (String, f32) {
        self.tickets.as_ref().and_then(|t| t.tickets.get((id as usize).saturating_sub(1))).map(|t| (t.name.clone(), t.value)).unwrap_or_default()
    }

    /// What a passenger asks for at the desk: the pack's ticket, or what a plugin made of it
    /// (its name, and the price a plugin set or took from the pack with the ticket). Nothing
    /// for somebody who neither buys nor has a ticket of the pack (a validator, a pass).
    pub fn ticket_request(&self, p: &Pax) -> (String, f32) {
        let pl = p.plugin();
        if p.ticket != TICKET_BUY && p.ticket_id == 0 && pl.by.is_none() {
            return (String::new(), 0.0);
        }
        let (name, value) = self.pack_ticket(p.ticket_id);
        let name = pl.name.clone().unwrap_or(name);
        (name, if pl.by.is_some() { p.price } else { value })
    }

    /// A stop's name as the timetable has it (what the passengers' destinations are made
    /// of), else the stop object's.
    pub fn stop_name(&self, stop: i64) -> Option<String> {
        let timetable = self.stop_names.as_ref().and_then(|n| n.get(&stop)).cloned();
        let own = || self.stops.get(&stop).map(|s| if s.alias.trim().is_empty() { s.name.clone() } else { s.alias.clone() });
        timetable.or_else(own).map(|n| n.trim().to_string())
    }

    /// Report passenger `i`'s ticket for the player's bus (they have just decided about it).
    pub fn ask_ticket(&mut self, i: usize) {
        if !self.ticket_hook || self.ticket_asks.len() >= MAX_ASKS {
            return;
        }
        let Some(p) = self.pax(i) else { return };
        let stop = p.stop;
        let destination = p.dest.as_ref().map(|d| d.trim().to_string());
        let (mut stop_index, mut destination_index) = (None, None);
        if let Some((trip, _, _)) = &self.duty {
            stop_index = stop.and_then(|s| trip.stops.iter().position(|(id, _, _)| *id == s));
            if let Some(d) = &destination {
                let from = stop_index.map_or(0, |k| k + 1);
                destination_index = trip.stops.iter().enumerate().skip(from).find(|(_, (_, n, _))| n.trim() == d.as_str()).map(|(k, _)| k);
            }
        }
        let (name, price) = self.ticket_request(p);
        let ask = TicketAsk {
            person: self.people[i].id,
            age: self.people[i].age,
            kind: p.ticket,
            ticket: p.ticket_id,
            name: name.trim().to_string(),
            price,
            stop,
            stop_name: stop.and_then(|s| self.stop_name(s)),
            destination,
            stop_index,
            destination_index,
        };
        let now = self.time;
        self.pax_mut(i).unwrap().ticket_hold = Some(now);
        self.ticket_asks.push(ask);
    }

    /// The passengers who decided about their ticket for the player's bus since the last
    /// call. They wait (`ticket_pending`) until `ticket_asks_heard`.
    pub fn take_ticket_asks(&mut self) -> Vec<TicketAsk> {
        std::mem::take(&mut self.ticket_asks)
    }

    /// The plugins have had their frame with these passengers (or never will: reports the
    /// game dropped): they go on now.
    pub fn ticket_asks_heard(&mut self, people: &[u32]) {
        if people.is_empty() {
            return;
        }
        for i in 0..self.people.len() {
            if people.contains(&self.people[i].id) {
                if let Some(p) = self.pax_mut(i) {
                    p.ticket_hold = None;
                }
            }
        }
    }

    /// Change passenger `person`'s ticket (`by`: the plugin). The kind changes only while
    /// they have not got in yet, the ticket only until they have asked for it at the desk;
    /// the tag always. A buyer always has a ticket of the pack: `ticket` 0 is refused for
    /// them, and one made a buyer without a ticket gets the pack's ticket for their age. The
    /// reason when nothing was changed.
    pub fn set_ticket(&mut self, person: u32, c: &TicketChange, by: &str) -> Result<(), &'static str> {
        let Some(i) = self.people.iter().position(|p| p.id == person) else { return Err("no such person") };
        let pack_len = self.tickets.as_ref().map_or(0, |t| t.tickets.len());
        let (has_sale, has_stamper) = self.player_cabin.as_ref().map_or((false, false), |c| (c.sale.is_some(), !c.stampers.is_empty()));
        let walk = self.boarding.eq_ignore_ascii_case("walk");
        let age = self.people[i].age;
        let Some(p) = self.pax(i) else { return Err("not a passenger") };
        if c.payment.is_some_and(|v| !v.is_finite() || v < 0.0) { return Err("invalid payment") }
        let mut ticket = c.ticket;
        let mut pick = false;
        let changes_ticket = c.changes_ticket();
        let changes_payment = c.payment.is_some();
        // (they decide on their way to the door: `new_ticket`)
        let outside = p.inside.is_none() && p.task == Task::WalkingToBus;
        let at_desk_yet = p.inside.is_some() && p.task == Task::InBusToPlace && p.ticket == TICKET_BUY && p.sub < 4;

        // The ticket and payment can only be changed for the player's bus,
        // while the passenger is boarding or on the way to the cash desk.
        if changes_ticket || changes_payment {
            if p.bus != Some(BusId::Player) {
                return Err("not a passenger of the player's bus");
            }
            // Paying only makes sense for somebody buying a ticket.
            // Use the requested kind when the plugin changes it in this call.
            if changes_payment && c.kind.unwrap_or(p.ticket) != TICKET_BUY {
                return Err("payment requires a ticket purchase");
            }

            if !outside && !at_desk_yet {
                return Err(if p.inside.is_some() { "already asked for the ticket" } else { "not boarding the bus yet" });
            }
            if changes_ticket {
                if let Some(kind) = c.kind.filter(|k| *k != p.ticket) {
                    if !outside {
                        return Err("already in the bus");
                    }
                    match kind {
                        TICKET_BUY if !has_sale || walk => return Err("the bus sells no tickets"),
                        TICKET_STAMP if !has_stamper => return Err("the bus has no validator"),
                        TICKET_NONE | TICKET_STAMP | TICKET_BUY => {}
                        _ => return Err("no such kind"),
                    }
                }
                if c.ticket.is_some_and(|t| t as usize > pack_len) {
                    return Err("no such ticket");
                }
                // a buyer without a ticket of the pack would wait at the desk for a ticket the
                // driver cannot give
                if c.kind.unwrap_or(p.ticket) == TICKET_BUY {
                    if c.ticket == Some(0) {
                        return Err("a buyer needs a ticket of the pack (from 1)");
                    }
                    pick = ticket.unwrap_or(p.ticket_id) == 0;
                }
            }
        }
        if pick {
            match self.pick_ticket(age) {
                Some(t) => ticket = Some((t + 1).min(255) as u8),
                None => return Err("no ticket of the pack is for their age: give one with `ticket`"),
            }
        }
        let pack_price = ticket.map(|t| self.pack_ticket(t).1);
        let p = self.pax_mut(i).unwrap();
        if changes_ticket {
            if let Some(k) = c.kind {
                p.ticket = k;
            }
            if let (Some(t), Some(v)) = (ticket, pack_price) {
                p.ticket_id = t;
                p.price = v;
            }
            if let Some(v) = c.price {
                p.price = if v.is_finite() { v.max(0.0) } else { 0.0 };
            }
            p.ticket_hold = None;
            let pl = p.plugin_mut();
            if ticket.is_some() {
                pl.name = None;
            }
            if let Some(n) = &c.name {
                pl.name = text(n);
            }
            if let Some(v) = &c.voice {
                pl.voice = text(v);
            }
            pl.by = text(by);
        }
        if let Some(payment) = c.payment {
            p.plugin_mut().payment = Some(payment);
        }
        if let Some(t) = &c.tag {
            p.plugin_mut().tag = text(t);
        }
        p.tidy_plugin();
        Ok(())
    }

    /// Say the ticket asked for: the plugin's line where the passenger's voice has it, else
    /// `Ticket_<id>_<k>`.
    pub fn say_ticket(&mut self, i: usize, id: usize, k: u64, custom: Option<&str>) {
        if let Some(line) = custom.filter(|l| self.voice_file(i, l).is_some()) {
            let line = line.to_string();
            self.say_line(i, &line, false, true);
            return;
        }
        self.say_line(i, &format!("Ticket_{id}_{k}"), false, true);
    }
}
