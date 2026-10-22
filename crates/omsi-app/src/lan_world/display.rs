pub(super) fn car_display(v: &omsi_sim::VehicleInstance) -> (String, String) {
    let line =
        v.ty.program
            .str_var("SetLineTo")
            .and_then(|i| v.state.str_vars.get(i as usize))
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
    let target = v
        .var("AI_target_index")
        .filter(|t| *t >= 0.0)
        .map(|t| t as i32)
        .unwrap_or(-1);
    (line, format!("#{target}"))
}

/// A host's timetable bus on our copy of it: line `line` and destination `destination`,
/// the row of the depot file [`car_display`] names (`#<row>`) - that row itself. Looked up
/// again by the row's sign text, it was the first row with that text anywhere on its sign:
/// Spandau's buses to U Ruhleben (282) showed Machandelweg (194), whose second line reads
/// RUHLEBEN.
pub(super) fn show_car_destination(
    v: &mut omsi_sim::VehicleInstance,
    hof: Option<&omsi_vehicle::Hof>,
    line: &str,
    destination: &str,
) {
    let row = destination
        .strip_prefix('#')
        .and_then(|n| n.parse::<usize>().ok());
    if let (Some(ti), Some(hof)) = (row, hof) {
        crate::schedule::set_ai_destination_at(v, hof, line, ti, &[]);
    }
}
