//! The host's AI script context: complete timetable pages and current system values.
//! Pages repeat in rotation; loss and reordering never expose a partial timetable.
use std::collections::HashMap;

pub const MAGIC: u8 = 0xB7;
const CHUNK: usize = 1000;
const HEADER: usize = 120;
const MAX_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Context {
    pub active: f32,
    pub line: String,
    pub terminus: i32,
    pub stops: Vec<(i64, String, f32, f32)>,
    pub index: i32,
    pub delay: f32,
    /// `omsi_script::SysVar::ALL` order.
    pub system: [f32; 22],
}

fn string(out: &mut Vec<u8>, s: &str) -> Option<()> {
    out.extend_from_slice(&u16::try_from(s.len()).ok()?.to_le_bytes());
    out.extend_from_slice(s.as_bytes());
    Some(())
}

impl Context {
    fn layout(&self) -> Option<Vec<u8>> {
        if !self.active.is_finite() || self.stops.len() > 4096 {
            return None;
        }
        let mut out = Vec::new();
        out.extend_from_slice(&self.active.to_le_bytes());
        out.extend_from_slice(&self.terminus.to_le_bytes());
        string(&mut out, &self.line)?;
        out.extend_from_slice(&(self.stops.len() as u16).to_le_bytes());
        for (id, name, arrival, departure) in &self.stops {
            if !arrival.is_finite() || !departure.is_finite() {
                return None;
            }
            out.extend_from_slice(&id.to_le_bytes());
            out.extend_from_slice(&arrival.to_le_bytes());
            out.extend_from_slice(&departure.to_le_bytes());
            string(&mut out, name)?;
            if out.len() > MAX_BYTES {
                return None;
            }
        }
        Some(out)
    }

    pub fn packet(&self, protocol: u8, id: u32, seq: u32, page: usize) -> Option<Vec<u8>> {
        if id > crate::world::MAX_ID
            || !self.delay.is_finite()
            || self.system.iter().any(|x| !x.is_finite())
        {
            return None;
        }
        let layout = self.layout()?;
        let count = layout.len().div_ceil(CHUNK);
        let part = page % count;
        let mut out = vec![MAGIC, protocol];
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&seq.to_le_bytes());
        out.extend_from_slice(&hash(&layout).to_le_bytes());
        out.extend_from_slice(&(layout.len() as u32).to_le_bytes());
        out.extend_from_slice(&(part as u16).to_le_bytes());
        out.extend_from_slice(&self.index.to_le_bytes());
        out.extend_from_slice(&self.delay.to_le_bytes());
        for v in self.system {
            out.extend_from_slice(&v.to_le_bytes());
        }
        debug_assert_eq!(out.len(), HEADER);
        out.extend_from_slice(&layout[part * CHUNK..((part + 1) * CHUNK).min(layout.len())]);
        Some(out)
    }
}

fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ *b as u64).wrapping_mul(0x100_0000_01b3)
    })
}

struct Read<'a>(&'a [u8]);
impl<'a> Read<'a> {
    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let out = self.0.get(..N)?.try_into().ok()?;
        self.0 = &self.0[N..];
        Some(out)
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take()?))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take()?))
    }
    fn f32(&mut self) -> Option<f32> {
        let v = f32::from_le_bytes(self.take()?);
        v.is_finite().then_some(v)
    }
    fn string(&mut self) -> Option<String> {
        let n = self.u16()? as usize;
        let s = std::str::from_utf8(self.0.get(..n)?).ok()?.to_owned();
        self.0 = &self.0[n..];
        Some(s)
    }
}

#[derive(Default)]
pub struct Receiver {
    entries: HashMap<u32, Assembly>,
}
struct Assembly {
    hash: u64,
    seq: u32,
    bytes: Vec<u8>,
    parts: Vec<bool>,
    dynamic: Context,
    layout: Option<Context>,
}

fn newer(seq: u32, old: u32) -> bool {
    seq != old && seq.wrapping_sub(old) < 0x8000_0000
}

impl Receiver {
    pub fn retain(&mut self, ids: &[u32]) {
        self.entries.retain(|id, _| ids.contains(id));
    }

    pub fn receive(&mut self, packet: &[u8], protocol: u8) -> Option<(u32, Context)> {
        if packet.len() < HEADER
            || packet.first() != Some(&MAGIC)
            || packet.get(1) != Some(&protocol)
        {
            return None;
        }
        let mut r = Read(&packet[2..]);
        let id = r.u32()?;
        if id > crate::world::MAX_ID {
            return None;
        }
        let seq = r.u32()?;
        let version = u64::from_le_bytes(r.take()?);
        let len = r.u32()? as usize;
        let part = r.u16()? as usize;
        let index = i32::from_le_bytes(r.take()?);
        let delay = r.f32()?;
        let mut system = [0.0; 22];
        for v in &mut system {
            *v = r.f32()?;
        }
        if len == 0
            || len > MAX_BYTES
            || part >= len.div_ceil(CHUNK)
            || r.0.len() != CHUNK.min(len - part * CHUNK)
        {
            return None;
        }
        let replace = match self.entries.get(&id) {
            Some(a) if a.hash != version => {
                if !newer(seq, a.seq) {
                    return None;
                }
                true
            }
            None => true,
            _ => false,
        };
        if replace {
            let reserved: usize = self.entries.values().map(|a| a.bytes.len()).sum();
            let old = self.entries.get(&id).map_or(0, |a| a.bytes.len());
            if reserved - old + len > 16 * 1024 * 1024
                || (self.entries.len() >= 1024 && !self.entries.contains_key(&id))
            {
                return None;
            }
            self.entries.insert(
                id,
                Assembly {
                    hash: version,
                    seq,
                    bytes: vec![0; len],
                    parts: vec![false; len.div_ceil(CHUNK)],
                    dynamic: Context {
                        index,
                        delay,
                        system,
                        ..Default::default()
                    },
                    layout: None,
                },
            );
        }
        let a = self.entries.get_mut(&id)?;
        if a.bytes.len() != len {
            return None;
        }
        if newer(seq, a.seq) {
            a.seq = seq;
            a.dynamic.index = index;
            a.dynamic.delay = delay;
            a.dynamic.system = system;
        }
        if a.layout.is_none() {
            a.bytes[part * CHUNK..part * CHUNK + r.0.len()].copy_from_slice(r.0);
            a.parts[part] = true;
            if !a.parts.iter().all(|p| *p) {
                return None;
            }
            if hash(&a.bytes) != a.hash {
                a.parts.fill(false);
                return None;
            }
            let mut r = Read(&a.bytes);
            let active = r.f32()?;
            let terminus = i32::from_le_bytes(r.take()?);
            let line = r.string()?;
            let count = r.u16()?;
            if count > 4096 {
                return None;
            }
            let mut stops = Vec::new();
            for _ in 0..count {
                let id = i64::from_le_bytes(r.take()?);
                let arrival = r.f32()?;
                let departure = r.f32()?;
                stops.push((id, r.string()?, arrival, departure));
            }
            if !r.0.is_empty() {
                return None;
            }
            a.layout = Some(Context {
                active,
                terminus,
                line,
                stops,
                ..Default::default()
            });
        }
        let mut context = a.layout.as_ref()?.clone();
        context.index = a.dynamic.index;
        context.delay = a.dynamic.delay;
        context.system = a.dynamic.system;
        Some((id, context))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn context() -> Context {
        Context {
            active: 1.0,
            line: "042".into(),
            terminus: 3,
            stops: (0..100)
                .map(|i| {
                    (
                        i,
                        format!("árvíztűrő megálló {i}"),
                        i as f32 * 60.0,
                        i as f32 * 60.0 + 10.0,
                    )
                })
                .collect(),
            index: 5,
            delay: -12.5,
            system: [42.0; 22],
        }
    }
    #[test]
    fn lost_reordered_pages_restore_a_complete_trip_and_latest_runtime() {
        let mut c = context();
        let count = c.layout().unwrap().len().div_ceil(CHUNK);
        let mut rx = Receiver::default();
        for p in (1..count).rev() {
            assert!(rx
                .receive(&c.packet(9, 7, p as u32, p).unwrap(), 9)
                .is_none());
        }
        c.index = 9;
        c.delay = 5.0;
        c.system[0] = 0.025;
        let (id, got) = rx.receive(&c.packet(9, 7, 100, 0).unwrap(), 9).unwrap();
        assert_eq!((id, got), (7, c.clone()));
        assert_eq!(rx.receive(&c.packet(9, 7, 1, 1).unwrap(), 9).unwrap().1, c);
    }
    #[test]
    fn changed_trips_reject_old_generations_and_inactive_context_clears_schedule() {
        let old = context();
        let mut rx = Receiver::default();
        for p in 0..10 {
            rx.receive(&old.packet(9, 7, p as u32, p).unwrap(), 9);
        }
        let cleared = Context::default();
        assert_eq!(
            rx.receive(&cleared.packet(9, 7, 100, 0).unwrap(), 9)
                .unwrap()
                .1,
            cleared
        );
        assert!(rx.receive(&old.packet(9, 7, 5, 0).unwrap(), 9).is_none());
        rx.retain(&[]);
        assert!(rx.entries.is_empty());
    }
    #[test]
    fn invalid_protocol_lengths_and_nonfinite_values_are_rejected() {
        let mut c = context();
        let p = c.packet(9, 7, 1, 0).unwrap();
        assert!(p.len() <= crate::MAX_DATAGRAM);
        assert!(Receiver::default().receive(&p, 8).is_none());
        assert!(Receiver::default().receive(&p[..p.len() - 1], 9).is_none());
        c.system[1] = f32::NAN;
        assert!(c.packet(9, 7, 1, 0).is_none());
    }
}
