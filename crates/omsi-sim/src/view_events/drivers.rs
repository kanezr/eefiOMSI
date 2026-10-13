//! What the drivers at the wheel of the timetable buses read of their bus, taken into the
//! [`FrameSnapshot`] at the view sync: where the bus stands, its variables (the wheel's turn,
//! the gear lever's) and its meshes' transforms (the wheel and the lever as they stand) -
//! so that the figures (the view's, `omsi_scene::driver`) are posed without reading the
//! simulation.

use super::*;
use glam::{DVec3, Mat4};
use hashbrown::HashMap;

/// A timetable bus near the viewer, as its driver reads it (see the module).
pub struct DriverFrame {
    /// The traffic's id of the bus.
    pub id: u64,
    pub ty: Arc<VehicleType>,
    pub position: DVec3,
    /// The body's rotation (heading, pitch and roll).
    pub rotation: Mat4,
    /// Variable names to slots of its variables.
    pub var_index: Arc<HashMap<String, omsi_script::VarId>>,
    vars: Span,
    transforms: Span,
}

/// The drivers' part of a [`FrameSnapshot`].
#[derive(Default)]
pub(super) struct Drivers {
    /// Where the viewer is (none: no drivers are posed).
    eye: Option<DVec3>,
    /// The traffic's last step (s).
    dt: f32,
    buses: Vec<DriverFrame>,
    vars: Vec<f32>,
    transforms: Vec<Mat4>,
}

impl FrameSnapshot {
    /// Where the viewer was when the drivers were taken (`TrafficSim::snapshot_drivers`).
    pub fn drivers_eye(&self) -> Option<DVec3> {
        self.drivers.eye
    }

    /// The traffic's last step (s), as the drivers move on it.
    pub fn drivers_dt(&self) -> f32 {
        self.drivers.dt
    }

    /// The timetable buses near the viewer, in the traffic's order.
    pub fn driver_buses(&self) -> &[DriverFrame] {
        &self.drivers.buses
    }

    /// The variable values of `bus`.
    pub fn driver_vars(&self, bus: &DriverFrame) -> &[f32] {
        &self.drivers.vars[bus.vars.range()]
    }

    /// The transforms of `bus`'s meshes (`VehicleInstance::mesh_transforms`).
    pub fn driver_transforms(&self, bus: &DriverFrame) -> &[Mat4] {
        &self.drivers.transforms[bus.transforms.range()]
    }
}

impl crate::ai_traffic::TrafficSim {
    /// The timetable buses within `within` m of the viewer as their drivers read them, into
    /// `out` (its earlier ones dropped). Nothing when there is no viewer.
    pub fn snapshot_drivers(&self, out: &mut FrameSnapshot, within: f64) {
        let d = &mut out.drivers;
        d.buses.clear();
        d.vars.clear();
        d.transforms.clear();
        d.eye = self.viewer.map(|v| v.pos);
        d.dt = self.last_dt;
        let Some(eye) = d.eye else { return };
        for c in &self.cars {
            if !c.is_bus() || c.gone || (c.vehicle.position - eye).length() > within {
                continue;
            }
            let v = &c.vehicle;
            d.buses.push(DriverFrame {
                id: c.id,
                ty: v.ty.clone(),
                position: v.position,
                rotation: v.body_rotation(),
                var_index: v.var_index().clone(),
                vars: Span::push(&mut d.vars, &v.state.vars),
                transforms: Span::push(&mut d.transforms, &v.mesh_transforms),
            });
        }
    }
}
