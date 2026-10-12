//! The bus's doors: which keys open which leaves, and the triggers the scripts' doors answer.

use super::*;

/// A door leaf's animation variable: `door_0`, `door_2L`, `Door_3` (and the stock scripts'
/// `doorTarget_0` names the same leaf).
fn door_leaf_of(var: &str) -> Option<String> {
    let v = var.to_ascii_lowercase();
    let rest = v.strip_prefix("doortarget_").or_else(|| v.strip_prefix("door_"))?;
    let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    let tail = &rest[digits..];
    (digits > 0 && tail.len() <= 1 && tail.chars().all(|c| c.is_ascii_alphabetic())).then(|| format!("door_{rest}"))
}

/// The door leaves a trigger moves: the leaf variables (or their targets) it stores, itself
/// or in its macros; failing that, the ones it reads.
pub(super) fn trigger_leaves(program: &omsi_script::Program, name: &str) -> Vec<String> {
    fn walk(program: &omsi_script::Program, block: omsi_script::BlockId, seen: &mut hashbrown::HashSet<omsi_script::BlockId>, stored: &mut Vec<String>, read: &mut Vec<String>) {
        if !seen.insert(block) || seen.len() > 64 {
            return;
        }
        let Some(b) = program.blocks.get(block as usize) else { return };
        for op in &b.ops {
            match op {
                omsi_script::Op::Store(id) | omsi_script::Op::Load(id) => {
                    let Some(leaf) = program.var_names.get(*id as usize).and_then(|n| door_leaf_of(n)) else { continue };
                    let list = if matches!(op, omsi_script::Op::Store(_)) { &mut *stored } else { &mut *read };
                    if !list.contains(&leaf) {
                        list.push(leaf);
                    }
                }
                omsi_script::Op::Macro(m) => walk(program, *m, seen, stored, read),
                _ => {}
            }
        }
    }
    let Some(b) = program.trigger(name) else { return Vec::new() };
    let (mut stored, mut read) = (Vec::new(), Vec::new());
    walk(program, b, &mut hashbrown::HashSet::new(), &mut stored, &mut read);
    if stored.is_empty() { read } else { stored }
}

/// The doors of a bus front to back, found from the model: the meshes each door leaf's
/// variable animates, and leaves standing within 1.6 m of each other along the bus are one
/// doorway. Each doorway gets the triggers that move its leaves: the toggles
/// (`bus_doorfront<n>`, `bus_dooraft`, a mod's own), or a mod's open and close pair
/// (`bus_door_0` / `bus_door_0_close`, written `open|close`: the one that fits the leaf's
/// state is fired). None when the model's doors cannot be told apart this way.
fn doorways(ty: &omsi_sim::VehicleType) -> Option<Vec<Vec<String>>> {
    // where each leaf is along the bus (the centre of the meshes it moves)
    let mut at: Vec<(String, f32, u32)> = Vec::new();
    for vm in &ty.meshes {
        let Some(def) = ty.model.meshes.get(vm.def_index) else { continue };
        let Some(leaf) = def.animations.iter().find_map(|a| door_leaf_of(&a.variable)) else { continue };
        if vm.data.positions.is_empty() {
            continue;
        }
        let y = vm.data.positions.iter().map(|p| vm.pivot.transform_point3(*p).y).sum::<f32>() / vm.data.positions.len() as f32;
        match at.iter_mut().find(|e| e.0 == leaf) {
            Some(e) => {
                e.1 += y;
                e.2 += 1;
            }
            None => at.push((leaf, y, 1)),
        }
    }
    if at.is_empty() {
        return None;
    }
    let mut leaves: Vec<(String, f32)> = at.into_iter().map(|(l, y, n)| (l, y / n as f32)).collect();
    leaves.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut ways: Vec<Vec<(String, f32)>> = Vec::new();
    for l in leaves {
        match ways.last_mut() {
            Some(w) if (w.last().unwrap().1 - l.1).abs() < 1.6 => w.push(l),
            _ => ways.push(vec![l]),
        }
    }
    let program = &ty.program;
    // which doorways a trigger reaches (its leaves' doorways; a script's branches all count)
    let way_of = |leaf: &String| ways.iter().position(|w| w.iter().any(|(l, _)| l == leaf));
    let reach = |name: &str| -> Vec<usize> {
        let mut v: Vec<usize> = trigger_leaves(program, name).iter().filter_map(way_of).collect();
        v.sort();
        v.dedup();
        v
    };
    // the stock keys by name (`bus_doorfront<n>`, two leaves of a doorway paired), a pair
    // split when its two triggers share no doorway: the LiAZ's `bus_doorfront0` and `1` are
    // its middle and rear doors, and Shift+1 opened both
    let mut groups: Vec<(Vec<String>, usize)> = Vec::new();
    for g in door_trigger_groups(program) {
        if g == ["bus_dooraft"] {
            continue;
        }
        let parts: Vec<Vec<String>> = match (g.first(), g.get(1)) {
            (Some(a), Some(b)) if !reach(a).is_empty() && !reach(b).is_empty() && !reach(a).iter().any(|w| reach(b).contains(w)) => vec![vec![a.clone()], vec![b.clone()]],
            _ => vec![g.clone()],
        };
        for part in parts {
            let front = part.iter().flat_map(|n| reach(n)).min().unwrap_or(usize::MAX);
            groups.push((part, front));
        }
    }
    // doorways no stock key reaches: a mod's own door triggers, all of whose leaves lie in
    // that doorway (a toggle, or an open and close pair written `open|close`)
    let reached: Vec<usize> = groups.iter().flat_map(|(g, _)| g.iter().flat_map(|n| reach(n))).collect();
    let mut names: Vec<&String> = program.triggers.keys().filter(|n| {
        let n = n.to_ascii_lowercase();
        (n.starts_with("bus_door") || n.starts_with("bus_tuer")) && !n.ends_with("_off") && !n.ends_with("_close") && !n.starts_with("bus_doorfront") && n != "bus_dooraft"
    }).collect();
    names.sort();
    for w in 0..ways.len() {
        if reached.contains(&w) {
            continue;
        }
        let mut group: Vec<String> = Vec::new();
        for n in &names {
            let r = reach(n);
            if r != [w] {
                continue;
            }
            let close = format!("{n}_close");
            group.push(if program.trigger(&close).is_some() { format!("{n}|{close}") } else { (*n).clone() });
        }
        if !group.is_empty() {
            groups.push((group, w));
        }
    }
    groups.sort_by_key(|(_, front)| *front);
    let mut out: Vec<Vec<String>> = groups.into_iter().map(|(g, _)| g).collect();
    // a release switch (the SD202's `bus_dooraft`) comes last
    if program.trigger("bus_dooraft").is_some() {
        out.push(vec!["bus_dooraft".to_string()]);
    }
    (!out.is_empty()).then_some(out)
}

/// The door keys of a vehicle type: its doorways from the model ([`doorways`]), else the
/// trigger names ([`door_trigger_groups`]).
pub(crate) fn door_keys(ty: &omsi_sim::VehicleType) -> Vec<Vec<String>> {
    doorways(ty).unwrap_or_else(|| door_trigger_groups(&ty.program))
}

/// Every door of a bus, `Shift+1` first: the stock scripts name a door `bus_doorfront<n>`
/// (the SD200/SD202/EN92 have two, both leaves of the one front door) and the aft one
/// `bus_dooraft` (which is also the stop brake release); a low-floor mod with three or four
/// independent doors, like the O530 Facelift, just has more `bus_doorfront<n>` triggers.
/// The keyboard names the passenger door, not each of its leaves.  Most stock and
/// add-on buses expose the two leaves as consecutive `bus_doorfront<n>` triggers.
/// A few older scripts use the second trigger as a *close* command instead; do not
/// combine that one with the opener (A3/BR275 are examples).
/// Sorted by `n` with `bus_dooraft` last. Consecutive leaves are grouped into one physical
/// doorway unless the script clearly names the second trigger as a close command.
pub(crate) fn door_trigger_groups(program: &omsi_script::Program) -> Vec<Vec<String>> {
    let mut fronts: Vec<(u32, String)> = program
        .triggers
        .keys()
        .filter_map(|n| {
            n.strip_prefix("bus_doorfront")
                .and_then(|rest| rest.parse::<u32>().ok())
                .map(|i| (i, n.clone()))
        })
        .collect();
    fronts.sort_by_key(|(i, _)| *i);
    let mut out = Vec::new();
    let mut i = 0;
    while i < fronts.len() {
        let mut group = vec![fronts[i].1.clone()];
        if let Some((next, name)) = fronts.get(i + 1) {
            if *next == fronts[i].0 + 1 && !door_trigger_closes(program, name) {
                group.push(name.clone());
                i += 1;
            }
        }
        out.push(group);
        i += 1;
    }
    if program.trigger("bus_dooraft").is_some() {
        out.push(vec!["bus_dooraft".to_string()]);
    }
    out
}

/// A door trigger that is a close command of its own (the A3's and BR275's second door key:
/// `1 (S.L.CCW_Tuerschliessen)`): a short block that itself stores into a variable named for
/// closing. Looked for in the macros as well, any mention of such a variable took a bus's
/// second door leaf for one - mod door scripts read `door_close_time` and the like while
/// opening - and Shift+1 moved one leaf, Shift+2 the other.
pub(crate) fn door_trigger_closes(program: &omsi_script::Program, name: &str) -> bool {
    let Some(b) = program.trigger(name).and_then(|b| program.blocks.get(b as usize)) else {
        return false;
    };
    if b.ops.len() > 6 || b.ops.iter().any(|op| matches!(op, omsi_script::Op::Macro(_))) {
        return false;
    }
    b.ops.iter().any(|op| match op {
        omsi_script::Op::Store(id) => {
            let n = program.var_names.get(*id as usize).map(|n| n.to_ascii_lowercase()).unwrap_or_default();
            ["close", "closing", "schliess", "schließ"].iter().any(|x| n.contains(x))
        }
        _ => false,
    })
}

/// The variable a door trigger toggles to say where the leaf is going (`doorTarget_0` of
/// the stock door scripts: the trigger's macros store it), if it has one.
pub(crate) fn door_trigger_target(program: &omsi_script::Program, name: &str) -> Option<omsi_script::VarId> {
    fn find(program: &omsi_script::Program, block: omsi_script::BlockId, seen: &mut hashbrown::HashSet<omsi_script::BlockId>) -> Option<omsi_script::VarId> {
        if !seen.insert(block) {
            return None;
        }
        let b = program.blocks.get(block as usize)?;
        for op in &b.ops {
            match op {
                omsi_script::Op::Store(id) => {
                    let n = program.var_names.get(*id as usize).map(|n| n.to_ascii_lowercase()).unwrap_or_default();
                    if n.contains("target") || n.contains("soll") {
                        return Some(*id);
                    }
                }
                omsi_script::Op::Macro(m) => {
                    if let Some(v) = find(program, *m, seen) {
                        return Some(v);
                    }
                }
                _ => {}
            }
        }
        None
    }
    program.trigger(name).and_then(|b| find(program, b, &mut hashbrown::HashSet::new()))
}

/// Which triggers of a door key's group to fire so that its leaves end up together: when
/// any is open (by its target), only the open ones (to close them), else all. Toggling
/// every leaf of a group made a closed leaf open while an open one closed.
pub(crate) fn door_group_to_fire(v: &mut omsi_sim::VehicleInstance, group: &[String]) -> Vec<String> {
    let fire = door_group_plan(v, group);
    if fire.len() < 2 {
        return fire;
    }
    // Triggers that undo each other are an open and a close key, not two leaves: Road-hog123's
    // door script (the UK buses' - the London Citybus 400, the Enviro400s) opens both leaves
    // on `bus_doorfront0` and closes both on `bus_doorfront1`, and firing the pair left the
    // doors shut: the passengers queued at the closed door for good. Tried on the scripts
    // first (the vehicle left as it was): when the doors' targets end where they began, only
    // the trigger that moves them now is fired.
    let mut targets: Vec<omsi_script::VarId> = group.iter().filter_map(|n| door_trigger_target(&v.ty.program, n.split('|').next().unwrap_or(n))).collect();
    targets.sort_unstable();
    targets.dedup();
    if targets.is_empty() {
        return fire;
    }
    let at = |vars: &[f32]| -> Vec<bool> { targets.iter().map(|&t| vars.get(t as usize).is_some_and(|x| *x > 0.5)).collect() };
    let base = at(&v.state.vars);
    let names: Vec<&str> = fire.iter().map(|s| s.as_str()).collect();
    if at(&v.trial_triggers(&names)) != base {
        return fire;
    }
    match fire.iter().find(|n| at(&v.trial_triggers(&[n.as_str()])) != base) {
        Some(one) => vec![one.clone()],
        None => fire,
    }
}

/// Whether the leaf door trigger `name` works is open now: what the trigger does to the
/// doors' targets when tried (a target it turns down was up), else the target its script
/// names first. The first one named can be a branch not taken: the SD202's
/// `bus_doorfront1` names the rear doors' target (`doorTarget_23`, for a bus whose rear
/// door the front buttons work) before its own leaf's, so with both front leaves open
/// the second Shift+1 shut only one of them.
pub(crate) fn door_trigger_open(v: &mut omsi_sim::VehicleInstance, name: &str) -> Option<bool> {
    let targets: Vec<usize> = v
        .ty
        .program
        .var_names
        .iter()
        .enumerate()
        .filter(|(_, n)| {
            let n = n.to_ascii_lowercase();
            n.contains("target") || n.contains("soll")
        })
        .map(|(k, _)| k)
        .collect();
    if !targets.is_empty() {
        let tried = v.trial_triggers(&[name]);
        let now = &v.state.vars;
        let changed = targets.iter().find(|&&k| (tried.get(k).copied().unwrap_or(0.0) > 0.5) != (now.get(k).copied().unwrap_or(0.0) > 0.5));
        if let Some(&k) = changed {
            return Some(now.get(k).copied().unwrap_or(0.0) > 0.5);
        }
    }
    door_trigger_target(&v.ty.program, name).and_then(|id| v.state.vars.get(id as usize).copied()).map(|x| x > 0.5)
}

/// Which triggers of a door key's group the doors' targets ask for (see
/// `door_group_to_fire`).
fn door_group_plan(v: &mut omsi_sim::VehicleInstance, group: &[String]) -> Vec<String> {
    // an open and close pair (`open|close`): whichever fits the leaf now
    let group: Vec<String> = group
        .iter()
        .map(|n| match n.split_once('|') {
            Some((open, close)) => {
                let leaf = trigger_leaves(&v.ty.program, open);
                let is_open = leaf.iter().any(|l| v.var(l).is_some_and(|x| x > 0.5));
                if is_open { close.to_string() } else { open.to_string() }
            }
            None => n.clone(),
        })
        .collect();
    let group = &group[..];
    let states: Vec<Option<bool>> = group
        .iter()
        .map(|n| door_trigger_open(v, n))
        .collect();
    if group.len() < 2 || states.iter().any(|s| s.is_none()) {
        return group.to_vec();
    }
    let any_open = states.iter().any(|s| *s == Some(true));
    group.iter().zip(&states).filter(|(_, s)| !any_open || **s == Some(true)).map(|(n, _)| n.clone()).collect()
}

/// `Digit1`..`Digit9` as 1..9 (`Digit0` and the numpad digits are left for whatever
/// `Inputs/keyboard.cfg` already puts on them).
pub(crate) fn digit_of(code: KeyCode) -> Option<usize> {
    Some(match code {
        KeyCode::Digit1 => 1,
        KeyCode::Digit2 => 2,
        KeyCode::Digit3 => 3,
        KeyCode::Digit4 => 4,
        KeyCode::Digit5 => 5,
        KeyCode::Digit6 => 6,
        KeyCode::Digit7 => 7,
        KeyCode::Digit8 => 8,
        KeyCode::Digit9 => 9,
        _ => return None,
    })
}

// (the game's own door actions by name live with the controllers: `omsi-input`)
pub(crate) use omsi_input::actions::door_action;
