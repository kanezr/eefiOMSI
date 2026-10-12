//! Variables of the vehicle's own scripts the engine reads or writes every frame by name
//! (the driver's seat, the text displays' refresh, the cabin air the scripts may keep
//! themselves, the radio's strings): looked up once per program, not as a lower-cased
//! copy and a hash look-up each per vehicle per frame. They are the program's - what
//! `program.var` / `program.str_var` answer - so they change only with the type
//! (`reload::retyped` makes them again).

use super::*;
use omsi_script::{StrVarId, VarId};

#[derive(Clone, Copy)]
pub(crate) struct ProgramIds {
    /// `Refresh_Strings`, cleared before the text textures are drawn.
    pub(super) refresh_strings: Option<VarId>,
    /// `Driver_Seat_VertTransl` (see `update_driver_seat`).
    pub(super) driver_seat: Option<VarId>,
    /// The scripts write `Cabinair_Temp` / `Cabinair_relHum` themselves (see
    /// `update_engine_vars`).
    pub(super) owns_cabin_temp: bool,
    pub(super) owns_cabin_rel_hum: bool,
    /// The radio's strings (see `show_radio_text`).
    pub(super) snd_radio_text: Option<StrVarId>,
    pub(super) magnitola: Option<StrVarId>,
    pub(super) mp3_freq: Option<StrVarId>,
    pub(super) mp3_track: Option<StrVarId>,
}

impl ProgramIds {
    fn of(p: &Program) -> ProgramIds {
        let owns = |name: &str| p.var(name).is_some_and(|v| p.stores(v));
        ProgramIds {
            refresh_strings: p.var("Refresh_Strings"),
            driver_seat: p.var("Driver_Seat_VertTransl"),
            owns_cabin_temp: owns("Cabinair_Temp"),
            owns_cabin_rel_hum: owns("Cabinair_relHum"),
            snd_radio_text: p.str_var("Snd_Radio_Text"),
            magnitola: p.str_var("magnitola_1"),
            mp3_freq: p.str_var("mp3_freq"),
            mp3_track: p.str_var("mp3_display_track_name"),
        }
    }
}

impl VehicleInstance {
    pub(super) fn program_ids(&self) -> &ProgramIds {
        self.program_ids.get_or_init(|| ProgramIds::of(&self.ty.program))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ids are what the names give, for a type that has the variables and one that has not.
    #[test]
    fn program_ids_answer_as_the_names() {
        let ty = crate::vehicle::tests::coupling_test_type(None);
        let v = VehicleInstance::new(ty, VehicleHost::new(Default::default()));
        let p = &v.ty.program;
        let ids = v.program_ids();
        assert_eq!(ids.refresh_strings, p.var("Refresh_Strings"));
        assert_eq!(ids.driver_seat, p.var("Driver_Seat_VertTransl"));
        assert_eq!(ids.owns_cabin_temp, p.var("Cabinair_Temp").is_some_and(|i| p.stores(i)));
        assert_eq!(ids.owns_cabin_rel_hum, p.var("Cabinair_relHum").is_some_and(|i| p.stores(i)));
        assert_eq!(ids.snd_radio_text, p.str_var("Snd_Radio_Text"));
        assert_eq!(ids.magnitola, p.str_var("magnitola_1"));
        assert_eq!(ids.mp3_freq, p.str_var("mp3_freq"));
        assert_eq!(ids.mp3_track, p.str_var("mp3_display_track_name"));
    }
}
