//! Requests made by human players, merged with simulated passengers every frame.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DoorRequest {
    pub bus: BusId,
    pub exit: bool,
    pub index: usize,
}

impl PeopleSim {
    /// Nearest entry from outside, or nearest exit of the bus the player is inside.
    /// Closed doors remain selectable: opening them is precisely what a request asks for.
    pub fn nearest_door_request(
        &self,
        at: DVec3,
        inside: Option<BusId>,
        reach: f64,
    ) -> Option<DoorRequest> {
        self.last_buses
            .iter()
            .filter(|b| inside.is_none_or(|id| b.id == id))
            .flat_map(|b| {
                let exit = inside.is_some();
                let doors = if exit {
                    &b.cabin.exits
                } else {
                    &b.cabin.entries
                };
                doors.iter().enumerate().map(move |(index, door)| {
                    let point = b.world(if exit { door.inside } else { door.outside });
                    (
                        (point - at).length(),
                        DoorRequest {
                            bus: b.id,
                            exit,
                            index,
                        },
                    )
                })
            })
            .filter(|(distance, _)| *distance <= reach)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, request)| request)
    }

    pub fn door_request_near(
        &self,
        request: DoorRequest,
        at: DVec3,
        inside: Option<BusId>,
        reach: f64,
    ) -> bool {
        if !at.is_finite()
            || request.exit != inside.is_some()
            || inside.is_some_and(|b| b != request.bus)
        {
            return false;
        }
        self.last_buses
            .iter()
            .find(|b| b.id == request.bus)
            .is_some_and(|b| {
                let doors = if request.exit {
                    &b.cabin.exits
                } else {
                    &b.cabin.entries
                };
                doors.get(request.index).is_some_and(|door| {
                    (b.world(if request.exit {
                        door.inside
                    } else {
                        door.outside
                    }) - at)
                        .length()
                        <= reach
                })
            })
    }

    pub fn requested_door_open(&self, request: DoorRequest) -> bool {
        self.last_buses
            .iter()
            .find(|b| b.id == request.bus)
            .is_none_or(|b| {
                let (entries, exits) = b
                    .walk_open
                    .as_ref()
                    .map(|(e, x)| (e, x))
                    .unwrap_or((&b.entry_open, &b.exit_open));
                (if request.exit { exits } else { entries })
                    .get(request.index)
                    .copied()
                    .unwrap_or(true)
            })
    }

    /// Source 0 is the local walker; LAN sources are their authenticated player IDs.
    /// Refreshing a lease survives packet loss but expires when its sender goes away.
    pub fn set_manual_door_request(&mut self, source: u32, request: DoorRequest, lease: f32) {
        if lease.is_finite() && lease > 0.0 {
            let new = self
                .manual_door_requests
                .get(&source)
                .is_none_or(|(old, _)| *old != request);
            if new && request.exit && request.bus == BusId::Player {
                self.stop_request = true;
            }
            self.manual_door_requests
                .insert(source, (request, lease.min(2.0)));
        }
    }

    pub fn clear_manual_door_request(&mut self, source: u32) {
        self.manual_door_requests.remove(&source);
    }

    pub(super) fn merge_manual_door_requests(&mut self, dt: f32, buses: &[BusNow]) {
        self.manual_door_requests.retain(|_, (request, remaining)| {
            if *remaining <= 0.0 {
                return false;
            }
            let Some(bus) = buses.iter().find(|b| b.id == request.bus) else {
                return false;
            };
            let (entries, exits) = bus
                .walk_open
                .as_ref()
                .map(|(e, x)| (e, x))
                .unwrap_or((&bus.entry_open, &bus.exit_open));
            let open = if request.exit { exits } else { entries };
            if open.get(request.index).copied().unwrap_or(true) {
                return false;
            }
            let Some((entry, exit)) = self.pax_req.get_mut(&request.bus) else {
                return false;
            };
            let flags = if request.exit { exit } else { entry };
            let Some(flag) = flags.get_mut(request.index) else {
                return false;
            };
            *flag = true;
            *remaining -= dt.max(0.0);
            true
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (PeopleSim, BusNow, VehicleInstance) {
        let dir =
            std::env::temp_dir().join(format!("omsi-human-door-request-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("test.bus"), "[model]\nmodel.cfg\n[script]\n1\nmain.osc\n[passengercabin]\ncabin.cfg\n[paths]\npaths.cfg\n").unwrap();
        std::fs::write(dir.join("model.cfg"), "").unwrap();
        std::fs::write(dir.join("main.osc"), "{init}\n{end}\n").unwrap();
        std::fs::write(
            dir.join("paths.cfg"),
            "[pathpnt]\n1.1\n4\n0.4\n[pathpnt]\n1.1\n-3\n0.4\n",
        )
        .unwrap();
        std::fs::write(dir.join("cabin.cfg"), "[entry]\n0\n[exit]\n1\n").unwrap();
        let ty = Arc::new(crate::VehicleType::load(&dir, &dir.join("test.bus")).unwrap());
        let mut vehicle = VehicleInstance::new(ty, crate::VehicleHost::new(Default::default()));
        let mut people = PeopleSim::new(&dir, 0);
        people.set_cabin(&mut vehicle);
        let cabin = people.cabin_for(&vehicle).unwrap();
        assert_eq!((cabin.entries.len(), cabin.exits.len()), (1, 1));
        let bus = BusNow {
            id: BusId::Player,
            cabin,
            pos: DVec3::new(100.0, 200.0, 0.0),
            rot: Mat4::from_rotation_z(-90f32.to_radians()),
            heading: 90.0,
            speed: 0.0,
            entry_open: vec![false],
            exit_open: vec![false],
            walk_open: None,
            interior: 0.0,
            air: CabinAir::default(),
            half: DVec2::new(1.25, 6.0),
            centre: DVec2::ZERO,
            accel: DVec2::ZERO,
            trailers: Vec::new(),
            terminus: None,
            takes: Takes::Nobody,
            next_stop: None,
            places_off: Vec::new(),
            served: None,
            route: None,
        };
        people.last_buses = Arc::new(vec![bus.clone()]);
        std::fs::remove_dir_all(dir).ok();
        (people, bus, vehicle)
    }

    #[test]
    fn human_requests_select_correct_points_merge_and_expire_without_erasing_passengers() {
        let (mut people, mut bus, mut vehicle) = fixture();
        let entry = bus.world(bus.cabin.entries[0].outside);
        let exit = bus.world(bus.cabin.exits[0].inside);
        let entry_req = DoorRequest {
            bus: BusId::Player,
            exit: false,
            index: 0,
        };
        let exit_req = DoorRequest {
            bus: BusId::Player,
            exit: true,
            index: 0,
        };
        assert_eq!(
            people.nearest_door_request(entry, None, 3.2),
            Some(entry_req)
        );
        assert_eq!(
            people.nearest_door_request(exit, Some(BusId::Player), 3.2),
            Some(exit_req)
        );
        assert!(people
            .nearest_door_request(exit, Some(BusId::Ai(7)), 3.2)
            .is_none());
        assert!(people
            .nearest_door_request(entry + DVec3::Z * 6.0, None, 3.2)
            .is_none());
        assert!(!people.door_request_near(exit_req, exit, None, 3.2));
        assert!(!people.door_request_near(entry_req, entry, Some(BusId::Player), 3.2));
        assert!(!people.door_request_near(entry_req, DVec3::splat(f64::NAN), None, 3.2));
        people.set_manual_door_request(2, entry_req, 1.0);
        people.set_manual_door_request(3, exit_req, 1.0);
        assert!(std::mem::take(&mut people.stop_request));
        people.set_manual_door_request(3, exit_req, 1.0);
        assert!(
            !people.stop_request,
            "lease refresh must not press the stop button again"
        );
        let buses = [bus.clone()];
        let frame = |people: &mut PeopleSim, dt| {
            people
                .pax_req
                .insert(BusId::Player, (vec![false], vec![false]));
            people.merge_manual_door_requests(dt, &buses);
            let (e, x) = people.pax_req.get(&BusId::Player).unwrap();
            people.entry_req = e.clone();
            people.exit_req = x.clone();
        };
        frame(&mut people, 0.5);
        people.write_pax_vars(&mut vehicle);
        assert_eq!(vehicle.var("PAX_Entry0_Req"), Some(1.0));
        assert_eq!(vehicle.var("PAX_Exit0_Req"), Some(1.0));
        frame(&mut people, 0.5);
        frame(&mut people, 0.1);
        people.write_pax_vars(&mut vehicle);
        assert_eq!(vehicle.var("PAX_Entry0_Req"), Some(0.0));
        assert_eq!(vehicle.var("PAX_Exit0_Req"), Some(0.0));
        // A simulated passenger still requests entry after the human lease expires.
        people
            .pax_req
            .insert(BusId::Player, (vec![true], vec![false]));
        people.merge_manual_door_requests(0.1, &buses);
        assert_eq!(people.pax_req[&BusId::Player].0, vec![true]);
        // Opening the door clears the human request, including a cancelled source.
        people.set_manual_door_request(3, exit_req, 1.0);
        bus.exit_open[0] = true;
        people.merge_manual_door_requests(0.1, &[bus.clone()]);
        assert!(!people.manual_door_requests.contains_key(&3));
        people.last_buses = Arc::new(vec![bus]);
        assert!(people.requested_door_open(exit_req));
        people.set_manual_door_request(3, entry_req, 1.0);
        people.clear_manual_door_request(3);
        assert!(people.manual_door_requests.is_empty());
        let ai_request = DoorRequest {
            bus: BusId::Ai(7),
            ..exit_req
        };
        people.set_manual_door_request(3, ai_request, 1.0);
        assert!(
            people.stop_wishes().0.contains(&7),
            "a human rider asks the AI to serve the next stop"
        );
    }
}
