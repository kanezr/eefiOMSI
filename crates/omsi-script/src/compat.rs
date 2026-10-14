//! Fixes for mod scripts that cannot show what they were written to show.
//!
//! Each fix is recognised by the script's own text, never by a file or bus name, and is
//! applied to the source lines before they are compiled.

/// The 4-character Annax line matrix of the LiAZ 5292 (a rewrite of the stock 3-character
/// `Matrix_D.osc`): a line number of one digit is first padded to four characters
/// (`4 $SetLengthL` → `"5   "`) and then cut to its last three (`3 $SetLengthR`, which
/// keeps the right end in OMSI too - see FORMATS.md), so line 5 came out blank and 5E as
/// `"   E"`. The number is written with three digits instead (`005E`, `051E`, `123E`), which
/// is what a three-digits-and-a-letter display shows.
///
/// The display has three cells for digits and a fourth for the letter, but the script
/// writes the letter lines Berlin style in front of the last digit or two (`" D" … 1
/// $SetLengthR " " $+ $+`): line 52D came out as `" D2 "`. Those lines are rewritten to
/// the number's three digits followed by the letter (`052D`), as the lines with a letter
/// behind the number (`10`, `30`…) already were.
fn four_char_matrix(lines: &mut [String]) -> bool {
    let has = |s: &str| lines.iter().any(|l| l.split_whitespace().collect::<Vec<_>>().join(" ") == s);
    if !(has("(L.$.Matrix_NewNr) $length 1 <=") && has("(L.$.Matrix_NewNr) 3 $SetLengthR \"E\" $+") && has("4 $SetLengthL")) {
        return false;
    }
    let mut changed = false;
    // whatever the letter code made of it, the number stored for the display goes through
    // one rule at the end: digits first, the letter in the fourth cell
    let n = lines.len();
    for i in 0..n {
        if lines[i].split_whitespace().collect::<Vec<_>>().join(" ") != "4 $SetLengthR" {
            continue;
        }
        let next = lines[i + 1..].iter().map(|l| l.trim()).find(|l| !l.is_empty());
        if next == Some("(S.$.Matrix_NewNr)") {
            let indent: String = lines[i].chars().take_while(|c| c.is_whitespace()).collect();
            lines[i] = format!("{indent}4 $SetLengthR $__DigitsFirst");
            changed = true;
        }
    }
    for l in lines.iter_mut() {
        if l.split_whitespace().collect::<Vec<_>>().join(" ") == "l1 trunc $IntToStr" {
            let indent: String = l.chars().take_while(|c| c.is_whitespace()).collect();
            *l = format!("{indent}l1 trunc \"03\" $IntToStrEnh");
            changed = true;
        } else if let Some(letter) = letter_in_front(l) {
            let indent: String = l.chars().take_while(|c| c.is_whitespace()).collect();
            *l = format!("{indent}(L.$.Matrix_NewNr) 3 $SetLengthR \"{letter}\" $+");
            changed = true;
        }
    }
    changed
}

/// Volvo Wright door scripts gate the rear close macro on the parking brake and on both rear door
/// leaves already being exactly `1`.  Their working outside-CL button bypasses those guards and
/// writes the close target directly.  Make the shared macro do the same while keeping the patch
/// specific to this known script shape.
fn volvo_rear_door_close(lines: &mut [String]) -> bool {
    let mut ranges = Vec::new();
    let mut start = None;
    for (i, line) in lines.iter().enumerate() {
        let text = line.trim();
        if text == "{macro:trg_bus_dooraftclose}" {
            start = Some(i + 1);
        } else if text == "{end}" {
            if let Some(begin) = start.take() {
                ranges.push((begin, i));
            }
        }
    }
    let mut changed = false;
    let mut recognised = false;
    for (start, end) in ranges {
        let body = &lines[start..end];
        let has = |needle: &str| body.iter().any(|line| line.trim() == needle);
        if !(has("(L.L.bremse_feststell_sw) 1 =")
            && has("(L.L.cockpit_button_smallhb) 1 = ||")
            && has("(L.L.door_2) 1 = &&")
            && has("(L.L.door_3) 1 = &&")
            && body.iter().any(|line| line.contains("S.L.doorTarget_23")))
        {
            continue;
        }
        recognised = true;
        for line in &mut lines[start..end] {
            if matches!(
                line.trim(),
                "(L.L.bremse_feststell_sw) 1 ="
                    | "(L.L.cockpit_button_smallhb) 1 = ||"
                    | "(L.L.door_2) 1 = &&"
                    | "(L.L.door_3) 1 = &&"
            ) {
                line.clear();
            }
        }
        lines[start] = "0 (S.L.bdoor_sound_played)".to_string();
        // The close animation emits ev_doortriggerclose_2 only when this latch is clear.
        // Some variants leave it set after the previous cycle, silencing the warning on the
        // next forced close.
        if start + 1 < end {
            lines[start + 1] = "1".to_string();
        }
        changed = true;
    }
    if recognised {
        // The forced-close button sets bdoor_embtn_cls while the leaves move.  The stock
        // warning-lamp condition unnecessarily excludes that state, so backdoor_buzzer never
        // reaches the model's mdoor_warn material on these variants.
        for i in 0..lines.len() {
            if lines[i].trim() != "1 (S.L.backdoor_buzzer)" {
                continue;
            }
            let begin = i.saturating_sub(12);
            if let Some(j) = (begin..i)
                .rev()
                .find(|&j| lines[j].trim() == "(L.L.bdoor_embtn_cls) 0 = &&")
            {
                lines[j].clear();
                changed = true;
            }
        }
    }
    changed
}

/// `"E" (L.$.Matrix_NewNr) 2 $SetLengthR " " $+ $+` or `" D" (L.$.Matrix_NewNr) 1
/// $SetLengthR " " $+ $+`: the letter written in front of the number.
fn letter_in_front(line: &str) -> Option<char> {
    let t = line.trim();
    let rest = t.strip_prefix('"')?;
    let (prefix, rest) = rest.split_once('"')?;
    let words: Vec<&str> = rest.split_whitespace().collect();
    if words.len() != 7
        || words[0] != "(L.$.Matrix_NewNr)"
        || !matches!(words[1], "1" | "2")
        || words[2] != "$SetLengthR"
        || words[3] != "\""
        || words[4] != "\""
        || words[5] != "$+"
        || words[6] != "$+"
    {
        return None;
    }
    let mut letters = prefix.chars().filter(|c| !c.is_whitespace());
    let letter = letters.next().filter(|c| c.is_ascii_alphabetic())?;
    letters.next().is_none().then_some(letter)
}

/// Apply every fix that recognises `lines`; returns the names of those applied.
pub fn patch(lines: &mut [String]) -> Vec<&'static str> {
    let mut applied = Vec::new();
    if four_char_matrix(lines) {
        applied.push("4-character line matrix: number padded to three digits");
    }
    if volvo_rear_door_close(lines) {
        applied.push("Volvo rear door closes while the leaves are moving");
    }
    applied
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liaz_matrix_pads_the_number() {
        let src = "\t\t\t\t\t\tl1 trunc $IntToStr\n(L.$.Matrix_NewNr) $length 1 <=\n4 $SetLengthL\n\t(L.$.Matrix_NewNr) 3 $SetLengthR \"E\" $+";
        let mut lines: Vec<String> = src.lines().map(String::from).collect();
        assert_eq!(patch(&mut lines).len(), 1);
        assert_eq!(lines[0], "\t\t\t\t\t\tl1 trunc \"03\" $IntToStrEnh");
        // the stock 3-character matrix is left alone
        let mut stock: Vec<String> = "l1 trunc $IntToStr\n(L.$.Matrix_NewNr) $length 1 <=\n2 $SetLengthL\n\"E\" (L.$.Matrix_NewNr) 2 $SetLengthR $+".lines().map(String::from).collect();
        assert!(patch(&mut stock).is_empty());
    }

    #[test]
    fn liaz_matrix_letter_behind_the_digits() {
        let src = "l1 trunc $IntToStr\n(L.$.Matrix_NewNr) $length 1 <=\n4 $SetLengthL\n(L.$.Matrix_NewNr) 3 $SetLengthR \"E\" $+\n\t\t\" D\" (L.$.Matrix_NewNr) 1 $SetLengthR \" \" $+ $+\n\"E\" (L.$.Matrix_NewNr) 2 $SetLengthR \" \" $+ $+\n\"BVG \"";
        let mut lines: Vec<String> = src.lines().map(String::from).collect();
        patch(&mut lines);
        assert_eq!(lines[4], "\t\t(L.$.Matrix_NewNr) 3 $SetLengthR \"D\" $+");
        assert_eq!(lines[5], "(L.$.Matrix_NewNr) 3 $SetLengthR \"E\" $+");
        assert_eq!(lines[6], "\"BVG \"");
    }

    #[test]
    fn volvo_rear_door_close_does_not_wait_for_both_leaves() {
        let src = "{macro:trg_bus_dooraftclose}\n(L.L.bremse_feststell_sw) 1 =\n(L.L.cockpit_button_smallhb) 1 = ||\n(L.L.door_2) 1 = &&\n(L.L.door_3) 1 = &&\n{if}\n0 (S.L.doorTarget_23)\n{endif}\n{end}\n{macro:other}\n(L.L.door_2) 1 = &&";
        let mut lines: Vec<String> = src.lines().map(String::from).collect();
        let applied = patch(&mut lines);
        assert!(applied.iter().any(|name| name.starts_with("Volvo rear door")));
        assert_eq!(lines[1], "0 (S.L.bdoor_sound_played)");
        assert_eq!(lines[2], "1");
        assert!(lines[3..5].iter().all(String::is_empty));
        assert_eq!(lines[10], "(L.L.door_2) 1 = &&");
    }

    #[test]
    fn volvo_rear_door_fix_requires_the_known_guard_shape() {
        let src = "{macro:trg_bus_dooraftclose}\n(L.L.door_2) 1 = &&\n(L.L.door_3) 1 = &&\n{end}";
        let mut lines: Vec<String> = src.lines().map(String::from).collect();
        assert!(patch(&mut lines).is_empty());
        assert_eq!(lines[1], "(L.L.door_2) 1 = &&");
    }

    #[test]
    fn volvo_rear_door_warning_allows_forced_close_state() {
        let src = "{macro:trg_bus_dooraftclose}\n(L.L.bremse_feststell_sw) 1 =\n(L.L.cockpit_button_smallhb) 1 = ||\n(L.L.door_2) 1 = &&\n(L.L.door_3) 1 = &&\n{if}\n0 (S.L.doorTarget_23)\n{endif}\n{end}\n(L.L.door_2) 0.1 >\n(L.L.doorTarget_23) 0 = &&\n(L.L.bdoor_embtn_cls) 0 = &&\n(L.L.doorbuzzer_timer) 0 >\n(L.L.doorbuzzer_timer) 1.3 < && ||\n{if}\n1 (S.L.backdoor_buzzer)\n{endif}";
        let mut lines: Vec<String> = src.lines().map(String::from).collect();
        assert_eq!(patch(&mut lines).len(), 1);
        assert!(!lines.iter().any(|line| line.trim() == "(L.L.bdoor_embtn_cls) 0 = &&"));
    }
}
