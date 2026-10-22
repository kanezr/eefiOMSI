//! Timestamped model animation inputs, evaluated on the remote bus's playback clock.

use hashbrown::{HashMap, HashSet};
use omsi_script::{Program, VarId};
use std::collections::VecDeque;

/// Include every animation reference, including cockpit meshes and coupled sections.
/// Constants and unresolved names have no numeric variable to synchronize.
pub fn model_variables<'a>(
    program: &Program,
    models: impl Iterator<Item = &'a omsi_model::Model>,
) -> HashSet<VarId> {
    models
        .flat_map(|m| &m.meshes)
        .flat_map(|m| &m.animations)
        .filter_map(|a| {
            let name = a.variable.trim();
            if name.parse::<f32>().is_ok() {
                None
            } else {
                program.var(name)
            }
        })
        .collect()
}

#[derive(Default)]
pub struct Animations {
    pub ids: HashSet<VarId>,
    samples: HashMap<VarId, VecDeque<(f64, f32)>>,
}

impl Animations {
    pub fn new(ids: HashSet<VarId>) -> Self {
        Self {
            ids,
            ..Default::default()
        }
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }

    /// A delayed or duplicate datagram must never undo a newer sample. Different
    /// packets of the same snapshot may name different variables at the same instant.
    pub fn push(&mut self, id: VarId, ms: u32, value: f32) -> bool {
        if !self.ids.contains(&id) || ms == 0 || !value.is_finite() {
            return false;
        }
        let t = ms as f64 / 1000.0;
        let samples = self.samples.entry(id).or_default();
        if samples.back().is_some_and(|s| t <= s.0) {
            return false;
        }
        samples.push_back((t, value));
        while samples.len() > 16 {
            samples.pop_front();
        }
        true
    }

    /// Stay within received states: do not guess beyond an endpoint (a door may stop
    /// or reverse), and do not wrap numeric variables merely because they drive rotation.
    pub fn value(&self, id: VarId, t: Option<f64>) -> Option<f32> {
        let samples = self.samples.get(&id)?;
        let t = t?;
        let first = samples.front()?;
        if t <= first.0 {
            return Some(first.1);
        }
        for (a, b) in samples.iter().zip(samples.iter().skip(1)) {
            if t <= b.0 {
                let f = ((t - a.0) / (b.0 - a.0)).clamp(0.0, 1.0);
                return Some((a.1 as f64 + (b.1 as f64 - a.1 as f64) * f) as f32);
            }
        }
        samples.back().map(|s| s.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doors_interpolate_and_other_variables_stay_immediate() {
        let mut a = Animations::new([7].into_iter().collect());
        assert!(a.push(7, 1000, 0.0));
        assert!(a.push(7, 1200, 1.0));
        assert_eq!(a.value(7, Some(0.5)), Some(0.0));
        assert_eq!(a.value(7, Some(1.1)), Some(0.5));
        assert_eq!(a.value(7, Some(2.0)), Some(1.0));
        assert!(!a.push(8, 1200, 1.0));
        assert_eq!(a.value(8, Some(1.1)), None);
    }

    #[test]
    fn stationary_samples_prevent_movement_spreading_over_a_long_pause() {
        let mut a = Animations::new([7].into_iter().collect());
        a.push(7, 1000, 0.0);
        a.push(7, 5000, 0.0);
        a.push(7, 5100, 1.0);
        assert_eq!(a.value(7, Some(4.9)), Some(0.0));
        assert!((a.value(7, Some(5.05)).unwrap() - 0.5).abs() < 1e-5);
        assert!(!a.push(7, 5050, 0.8));
        assert!(!a.push(7, 5100, 0.8));
        assert!(!a.push(7, 5200, f32::NAN));
        assert_eq!(a.value(7, Some(5.2)), Some(1.0));
        a.clear();
        assert_eq!(a.value(7, Some(5.2)), None);
        assert!(a.push(7, 100, 0.2));
    }

    #[test]
    fn model_references_include_all_meshes_and_trailers_without_duplicates() {
        let mut program = Program::default();
        let door = program.declare_var("door");
        let cockpit = program.declare_var("switch");
        let rear = program.declare_var("rear");
        let model = |names: &[&str]| omsi_model::Model {
            meshes: vec![omsi_model::MeshDef {
                animations: names
                    .iter()
                    .map(|name| omsi_model::Animation {
                        variable: name.to_string(),
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let front = model(&[" door ", "switch", "DOOR", "1", "missing"]);
        let trailer = model(&["rear"]);
        assert_eq!(
            model_variables(&program, [&front, &trailer].into_iter()),
            [door, cockpit, rear].into_iter().collect()
        );
    }
}
