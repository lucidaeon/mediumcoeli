//! Plain-text fdupes-style screen for LUNA consolidation.
//!
//! One group at a time: header line ("Group N of M — K candidates"), one
//! row per chart in the group with index + name + date + time + lat/long +
//! phenom_id, then a prompt line.  User keystrokes are read line-by-line
//! from stdin; each non-Skip choice is persisted via [`DecisionLog`]
//! before the loop advances.

use astrogram::chart::Chart;
use astrogram::decision_log::{Choice, DecisionLog, DecisionLogError, DecisionRecord};
use std::fmt::Write as _;
use std::io::{BufRead, Write};

/// Render one group as a multi-line screen.
///
/// `group_no` and `group_total` are 1-based for human display.  `indices`
/// are positions into `charts`.  `phenom_ids` parallels `charts`.
#[must_use]
pub fn render_group(
    group_no: usize,
    group_total: usize,
    indices: &[usize],
    charts: &[Chart],
    phenom_ids: &[String],
) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "\n── Group {group_no} of {group_total} — {} candidates ──",
        indices.len()
    );
    for (slot, &idx) in indices.iter().enumerate() {
        let c = &charts[idx];
        let date = format!("{:04}-{:02}-{:02}", c.year, c.month, c.day);
        let time = format!("{:02}:{:02}:{:02}", c.hour, c.minute, c.second);
        let lat = c.latitude.degrees();
        let lon = c.longitude.degrees();
        let pid = phenom_ids.get(idx).map_or("", String::as_str);
        let label = letter_for_slot(slot);
        let name = &c.name;
        let _ = writeln!(
            out,
            "  [{label}] {name}\n      {date} {time}  lat {lat:>8.4}  lon {lon:>9.4}  {pid}"
        );
    }
    out.push_str("  Keep one [a-z] (others auto-drop), (s)kip group, (q)uit > ");
    out
}

fn letter_for_slot(slot: usize) -> char {
    if slot < 26 {
        // slot < 26 guarantees the cast fits in u8.
        #[allow(clippy::cast_possible_truncation)]
        let offset = slot as u8;
        (b'a' + offset) as char
    } else {
        '?'
    }
}

/// One parsed user keystroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    /// User picked slot `n` (0-based) to keep.  All others in the group
    /// become Drop.
    KeepOnly(usize),
    /// User explicitly marked one slot as Drop (others stay undecided).
    #[allow(dead_code)] // reserved for future per-slot Drop flow
    DropOne(usize),
    /// Skip this group (re-prompt next run).
    SkipGroup,
    /// Quit the consolidation flow; apply phase still runs over decisions
    /// already logged.
    Quit,
}

/// Parse one user input line.  Returns `None` when the input is empty or
/// uninterpretable so the caller can re-prompt.
#[must_use]
pub fn parse_input(line: &str) -> Option<Input> {
    let s = line.trim();
    if s.len() != 1 {
        return None;
    }
    let c = s.chars().next().unwrap();
    match c {
        'q' => Some(Input::Quit),
        's' => Some(Input::SkipGroup),
        'a'..='z' => Some(Input::KeepOnly((c as u8 - b'a') as usize)),
        // Uppercase letters are reserved for a future per-slot Drop flow
        // (see `Input::DropOne`).  V1 uses KeepOnly + auto-Drop-the-rest
        // so the user can't get stuck cycling through Drops with no exit.
        _ => None,
    }
}

/// Outcome of running the consolidation loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    /// Reached the end of the groups.
    Completed,
    /// User pressed `q` mid-flow.
    QuitEarly,
}

/// Walk groups, prompting for each, persisting every non-Skip decision
/// before advancing.
///
/// `already_decided` is the set of `group_id`s the caller has already
/// resolved from a prior log read; matching groups are silently skipped.
///
/// # Errors
/// - [`DecisionLogError::Io`] / [`DecisionLogError::Json`] on log write
///   failure.  Reading input never errors; an unparseable line re-prompts.
pub fn run_loop(
    groups: &[Vec<usize>],
    charts: &[Chart],
    phenom_ids: &[String],
    already_decided: &std::collections::HashSet<String>,
    log: &mut DecisionLog,
    stdin: &mut dyn BufRead,
    stdout: &mut dyn Write,
) -> Result<RunOutcome, DecisionLogError> {
    let total = groups.len();
    for (g_idx, group) in groups.iter().enumerate() {
        let group_id = group_id_for(group, phenom_ids);
        if already_decided.contains(&group_id) {
            continue;
        }
        loop {
            let screen = render_group(g_idx + 1, total, group, charts, phenom_ids);
            stdout.write_all(screen.as_bytes())?;
            stdout.flush()?;

            let mut line = String::new();
            if stdin.read_line(&mut line)? == 0 {
                return Ok(RunOutcome::QuitEarly);
            }
            match parse_input(&line) {
                Some(Input::Quit) => return Ok(RunOutcome::QuitEarly),
                Some(Input::SkipGroup) => {
                    stdout.write_all(b"  skipped\n")?;
                    break;
                }
                Some(Input::KeepOnly(slot)) if slot < group.len() => {
                    persist_keep_only(group, slot, &group_id, charts, phenom_ids, log)?;
                    break;
                }
                _ => {
                    stdout.write_all(b"  ?\n")?;
                }
            }
        }
    }
    Ok(RunOutcome::Completed)
}

fn group_id_for(group: &[usize], phenom_ids: &[String]) -> String {
    for &idx in group {
        let pid = phenom_ids.get(idx).map_or("", String::as_str);
        if !pid.is_empty() {
            return pid.to_string();
        }
    }
    format!(
        "synthetic-{}-{}",
        group.first().copied().unwrap_or(0),
        group.len()
    )
}

fn persist_keep_only(
    group: &[usize],
    keep_slot: usize,
    group_id: &str,
    charts: &[Chart],
    phenom_ids: &[String],
    log: &mut DecisionLog,
) -> Result<(), DecisionLogError> {
    for (slot, &idx) in group.iter().enumerate() {
        let choice = if slot == keep_slot {
            Choice::Keep
        } else {
            Choice::Drop
        };
        let pid = phenom_ids.get(idx).map_or("", String::as_str).to_string();
        log.append(&DecisionRecord {
            group_id: group_id.to_string(),
            phenom_id: pid,
            choice,
            chart_name: charts[idx].name.clone(),
        })?;
    }
    Ok(())
}

#[allow(dead_code)] // reserved for future per-slot Drop flow
fn persist_drop_one(
    group: &[usize],
    drop_slot: usize,
    group_id: &str,
    charts: &[Chart],
    phenom_ids: &[String],
    log: &mut DecisionLog,
) -> Result<(), DecisionLogError> {
    let idx = group[drop_slot];
    let pid = phenom_ids.get(idx).map_or("", String::as_str).to_string();
    log.append(&DecisionRecord {
        group_id: group_id.to_string(),
        phenom_id: pid,
        choice: Choice::Drop,
        chart_name: charts[idx].name.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use astrogram::chart::{
        Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, Zodiac,
    };

    fn ch(name: &str, lat: f64, lon: f64, y: i16, mo: u8, d: u8) -> Chart {
        Chart {
            name: name.into(),
            secondary_name: None,
            city: None,
            region: None,
            longitude: Longitude::new(lon).unwrap(),
            latitude: Latitude::new(lat).unwrap(),
            year: y,
            month: mo,
            day: d,
            hour: 12,
            minute: 0,
            second: 0,
            tz_offset_hours: 0.0,
            tz_abbreviation: None,
            is_lmt: false,
            event_type: EventType::Unspecified,
            source_rating: None,
            house_system: HouseSystem::Placidus,
            zodiac: Zodiac::Tropical,
            coordinate_system: CoordinateSystem::Geocentric,
            sub_charts: vec![],
            notes: None,
        }
    }

    #[test]
    fn parse_input_understands_keep_skip_and_quit_letters() {
        assert_eq!(parse_input("a"), Some(Input::KeepOnly(0)));
        assert_eq!(parse_input("c"), Some(Input::KeepOnly(2)));
        assert_eq!(parse_input("s"), Some(Input::SkipGroup));
        assert_eq!(parse_input("q"), Some(Input::Quit));
    }

    #[test]
    fn parse_input_rejects_uppercase_for_now() {
        // Uppercase is reserved for a future per-slot Drop flow.
        assert_eq!(parse_input("A"), None);
        assert_eq!(parse_input("B"), None);
        assert_eq!(parse_input("Z"), None);
    }

    #[test]
    fn parse_input_rejects_garbage() {
        assert_eq!(parse_input(""), None);
        assert_eq!(parse_input("  "), None);
        assert_eq!(parse_input("ab"), None);
        assert_eq!(parse_input("1"), None);
        assert_eq!(parse_input("?"), None);
    }

    #[test]
    fn parse_input_strips_trailing_newline() {
        assert_eq!(parse_input("a\n"), Some(Input::KeepOnly(0)));
        assert_eq!(parse_input("q\r\n"), Some(Input::Quit));
    }

    #[test]
    fn render_group_lists_every_candidate_with_slot_letter() {
        let charts = vec![
            ch("A", 40.0, -75.0, 1990, 5, 1),
            ch("B verbose", 40.0, -75.0, 1990, 5, 1),
        ];
        let ids = vec!["uuid-a".to_string(), "uuid-b".to_string()];
        let s = render_group(1, 3, &[0, 1], &charts, &ids);
        assert!(s.contains("Group 1 of 3"));
        assert!(s.contains("[a] A"));
        assert!(s.contains("[b] B verbose"));
        assert!(s.contains("uuid-a"));
        assert!(s.contains("uuid-b"));
        assert!(s.contains("Keep one [a-z] (others auto-drop), (s)kip group, (q)uit"));
    }
}
