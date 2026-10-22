//! Name manifests for safe variable mapping between compatible bus variants.
//! Transport uses the same versioned, recoverable string fragments as VARS, on a
//! separate channel so schema fragments cannot discard in-flight script strings.

pub const MAGIC: u8 = 0xB8;
const MAX_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Schema {
    pub floats: Vec<String>,
    pub strings: Vec<String>,
    /// Sorted, unique typed references (`f:name`, `s:name`, `u:unresolved`).
    pub model: Vec<String>,
    pub sound: Vec<String>,
}

impl Schema {
    pub fn table_hash(&self) -> u32 {
        let mut h: u32 = 0x811c_9dc5;
        for (kind, names) in [(0u8, &self.floats), (1u8, &self.strings)] {
            for b in std::iter::once(kind).chain(
                names
                    .iter()
                    .flat_map(|n| n.bytes().chain(std::iter::once(0))),
            ) {
                h = (h ^ b as u32).wrapping_mul(0x0100_0193);
            }
        }
        h
    }

    pub fn compatible(&self, other: &Self) -> bool {
        let resolved = |s: &Self| {
            s.model.iter().chain(&s.sound).all(|reference| {
                let Some((kind, name)) = reference.split_once(':') else {
                    return false;
                };
                match kind {
                    "f" => s.floats.binary_search_by(|n| n.as_str().cmp(name)).is_ok(),
                    "s" => s.strings.binary_search_by(|n| n.as_str().cmp(name)).is_ok(),
                    // References absent from both programs still compare by name. Some
                    // model keys name engine-owned values or repaint selectors rather
                    // than script slots; they must not disable otherwise safe mapping.
                    "u" => {
                        name != "unreadable_sound_configuration"
                            && !s.floats.iter().chain(&s.strings).any(|n| n == name)
                    }
                    _ => false,
                }
            })
        };
        self.model == other.model && self.sound == other.sound && resolved(self) && resolved(other)
    }

    pub fn encode(&self) -> Option<String> {
        let mut out = String::new();
        for list in [&self.floats, &self.strings, &self.model, &self.sound] {
            out.push_str(&format!("{}:", list.len()));
            for name in list {
                out.push_str(&format!("{}:", name.len()));
                out.push_str(name);
            }
        }
        (out.len() <= MAX_BYTES).then_some(out)
    }

    pub fn decode(mut input: &str) -> Option<Self> {
        if input.len() > MAX_BYTES {
            return None;
        }
        fn number(input: &mut &str) -> Option<usize> {
            let (n, rest) = input.split_once(':')?;
            if n.len() > 7 || n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            *input = rest;
            n.parse().ok()
        }
        fn list(input: &mut &str) -> Option<Vec<String>> {
            let count = number(input)?;
            if count > 65536 || count > input.len() / 2 {
                return None;
            }
            let mut out = Vec::with_capacity(count);
            for _ in 0..count {
                let len = number(input)?;
                let name = input.get(..len)?;
                if name.is_empty() || name != name.to_ascii_lowercase() {
                    return None;
                }
                out.push(name.to_owned());
                *input = input.get(len..)?;
            }
            if !out.windows(2).all(|p| p[0] < p[1]) {
                return None;
            }
            Some(out)
        }
        let schema = Self {
            floats: list(&mut input)?,
            strings: list(&mut input)?,
            model: list(&mut input)?,
            sound: list(&mut input)?,
        };
        input.is_empty().then_some(schema)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codec_and_configuration_compatibility() {
        let a = Schema {
            floats: vec!["door".into(), "internal".into()],
            strings: vec!["display".into()],
            model: vec!["f:door".into(), "s:display".into()],
            sound: vec!["f:door".into()],
        };
        assert_eq!(Schema::decode(&a.encode().unwrap()), Some(a.clone()));
        let mut b = a.clone();
        b.floats.push("variant_only".into());
        assert_ne!(a.table_hash(), b.table_hash());
        assert!(a.compatible(&b));
        b.sound.push("f:internal".into());
        assert!(!a.compatible(&b));
        b = a.clone();
        b.floats.remove(0);
        assert!(!a.compatible(&b));
        for text in [
            "9999999:",
            "1:2:é0:0:0:garbage",
            "1:1:A0:0:0:",
            "2:1:a1:a0:0:0:",
        ] {
            assert!(Schema::decode(text).is_none());
        }
    }

    #[test]
    fn schema_recovers_lost_fragments_without_disturbing_values() {
        let schema = Schema {
            floats: (0..2000).map(|i| format!("variable_{i:04}")).collect(),
            ..Default::default()
        };
        let text = schema.encode().unwrap();
        let mut sender = crate::vars::VarSender::default();
        let mut receiver = crate::vars::VarReceiver::default();
        let mut got = None;
        let mut lost = false;
        for _ in 0..2000 {
            for packet in sender.tick(
                10,
                2,
                schema.table_hash(),
                &[],
                &[],
                &[0],
                std::slice::from_ref(&text),
                0.1,
            ) {
                if packet[10] == 3 && !lost {
                    lost = true;
                    continue;
                }
                if let Some(v) = receiver.receive(&packet, 10) {
                    for (_, value) in v.strings {
                        got = Schema::decode(&value);
                    }
                }
            }
            if got.is_some() {
                break;
            }
        }
        assert!(lost);
        assert_eq!(got, Some(schema));
    }
}
