use anyhow::{Context, Result};
use astrogram::astrocom::AstrocomSession;
use astrogram::astrotheoros::{AstrotheorosSession, entry_to_chart};
use astrogram::capability::ChartField;
use astrogram::chart::{Chart, CoordinateSystem, HouseSystem, Zodiac};
use astrogram::luna::LunaSession;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::{IsTerminal, Write as _};

pub type DatetimeKey = (String, i16, u8, u8, u8, u8, u8);

/// Per-record callback for [`WebProvider::write_charts`] on inline-verify
/// providers: `(new_index, total_new, source, landed, status)`.  `landed` is
/// `None` if the create failed.
pub type LandedFn<'a> = dyn FnMut(usize, usize, &Chart, Option<&Chart>, &str) + 'a;

pub fn key(c: &Chart) -> DatetimeKey {
    (
        c.name.clone(),
        c.year,
        c.month,
        c.day,
        c.hour,
        c.minute,
        c.second,
    )
}

/// A sink's account-wide render settings, folded into landed charts before
/// diffing. `field_notes` carries provenance notes (e.g. `"global setting"`,
/// `"not supported"`) for the transcript.
pub struct GlobalRender {
    /// Global house system the sink renders with.
    pub house_system: HouseSystem,
    /// Global zodiac the sink renders with.
    pub zodiac: Zodiac,
    /// Coordinate system the sink renders with (astrotheoros: always geocentric).
    pub coordinate_system: CoordinateSystem,
    /// Per-field provenance notes for the transcript.
    pub field_notes: Vec<(ChartField, &'static str)>,
}

pub enum WebProvider {
    Luna {
        session: LunaSession,
        /// Stored at construction from cli.luna_resume_from; used by read_input().
        resume_from: Option<String>,
        /// Stored at construction from cli.normalize; used by read_input().
        normalize: bool,
        /// Populated by read_existing(): listing keys for create-only dedup.
        listing_keys: HashSet<DatetimeKey>,
        /// Populated by read_input(): phenom_ids for update-in-place.
        phenom_ids: Vec<String>,
    },
    Astrocom {
        session: AstrocomSession,
        /// Populated when login path was used; needed by delete_one().
        /// None when token-only — delete_one() bails clearly in that case.
        creds: Option<(String, String)>,
        /// Populated by read_existing() or read_input().
        nhor_id_map: HashMap<DatetimeKey, u32>,
    },
    Astrotheoros {
        session: AstrotheorosSession,
        /// Populated by read_existing() or read_input().
        uuid_map: HashMap<DatetimeKey, String>,
    },
}

impl std::fmt::Debug for WebProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Luna { .. } => f.write_str("WebProvider::Luna { .. }"),
            Self::Astrocom { .. } => f.write_str("WebProvider::Astrocom { .. }"),
            Self::Astrotheoros { .. } => f.write_str("WebProvider::Astrotheoros { .. }"),
        }
    }
}

impl WebProvider {
    /// Read existing content from the target for dedup purposes.
    ///
    /// Luna: fetches listing keys only (no per-chart HTTP); returns [].
    /// Astrocom / Astrotheoros: fetches all charts; returns them.
    pub fn read_existing(&mut self) -> Result<Vec<Chart>> {
        match self {
            WebProvider::Luna {
                session,
                listing_keys,
                ..
            } => {
                println!("LUNA (existing): reading…");
                let rows = session.fetch_listing().context("reading LUNA listing")?;
                println!("Found {} charts in LUNA", rows.len());
                *listing_keys = rows
                    .into_iter()
                    .map(|r| (r.name, r.year, r.month, r.day, r.hour, r.minute, r.second))
                    .collect();
                Ok(vec![])
            }
            WebProvider::Astrocom {
                session,
                nhor_id_map,
                ..
            } => {
                println!("astro.com (existing): reading…");
                let (charts, ids) = session
                    .fetch_charts()
                    .context("fetching astro.com charts")?;
                println!("astro.com: {} charts (existing)", charts.len());
                *nhor_id_map = charts
                    .iter()
                    .zip(ids.iter())
                    .map(|(c, &id)| (key(c), id))
                    .collect();
                Ok(charts)
            }
            WebProvider::Astrotheoros {
                session, uuid_map, ..
            } => {
                println!("astrotheoros.com (existing): reading…");
                let (charts, uuids) = session
                    .fetch_charts()
                    .context("fetching astrotheoros.com charts")?;
                println!("astrotheoros.com: {} charts (existing)", charts.len());
                *uuid_map = charts
                    .iter()
                    .zip(uuids.iter())
                    .map(|(c, u)| (key(c), u.clone()))
                    .collect();
                Ok(charts)
            }
        }
    }

    /// Read charts from this target as an input source.
    ///
    /// All variants: full chart fetch. Populates internal ID cache.
    pub fn read_input(&mut self) -> Result<Vec<Chart>> {
        match self {
            WebProvider::Luna {
                session,
                resume_from,
                normalize,
                phenom_ids,
                ..
            } => {
                let (charts, ids) = session
                    .fetch_charts(
                        resume_from.as_deref(),
                        *normalize,
                        &|i, total, name| {
                            print!("[{i:>3}/{total}] {name:<40}  ");
                            let _ = std::io::stdout().flush();
                        },
                        &|status| {
                            if !status.is_empty() {
                                println!("{status}");
                            }
                        },
                    )
                    .context("fetching LUNA charts")?;
                *phenom_ids = ids;
                Ok(charts)
            }
            WebProvider::Astrocom {
                session,
                nhor_id_map,
                ..
            } => {
                let (charts, ids) = session
                    .fetch_charts()
                    .context("fetching astro.com charts")?;
                *nhor_id_map = charts
                    .iter()
                    .zip(ids.iter())
                    .map(|(c, &id)| (key(c), id))
                    .collect();
                Ok(charts)
            }
            WebProvider::Astrotheoros {
                session, uuid_map, ..
            } => {
                let (charts, uuids) = session
                    .fetch_charts()
                    .context("fetching astrotheoros.com charts")?;
                *uuid_map = charts
                    .iter()
                    .zip(uuids.iter())
                    .map(|(c, u)| (key(c), u.clone()))
                    .collect();
                Ok(charts)
            }
        }
    }

    /// Fetch the sink's account-wide render settings, if it has any.
    ///
    /// astrotheoros stores house system / zodiac globally; luna and astro.com
    /// return `Ok(None)` (their settings model is not yet wired).
    pub fn fetch_global_settings(&self) -> Result<Option<GlobalRender>> {
        match self {
            WebProvider::Astrotheoros { session, .. } => {
                let s = session
                    .fetch_settings()
                    .context("fetching astrotheoros.com settings")?;
                Ok(Some(GlobalRender {
                    house_system: s.house_system,
                    zodiac: s.zodiac,
                    coordinate_system: CoordinateSystem::Geocentric,
                    field_notes: vec![
                        (ChartField::HouseSystem, "global setting"),
                        (ChartField::Zodiac, "global setting"),
                        (ChartField::CoordinateSystem, "not supported"),
                    ],
                }))
            }
            WebProvider::Luna { .. } | WebProvider::Astrocom { .. } => Ok(None),
        }
    }

    pub fn site_display(&self) -> &'static str {
        match self {
            WebProvider::Luna { .. } => "LUNA",
            WebProvider::Astrocom { .. } => "astro.com",
            WebProvider::Astrotheoros { .. } => "astrotheoros.com",
        }
    }

    /// Write charts to this target.
    ///
    /// Uses the ID cache populated by read_existing() or read_input().
    ///
    /// Luna listing-keys mode (came from read_existing): filters `charts`
    /// against listing_keys, creates only charts not already present.
    /// Luna phenom-ids mode (came from read_input, normalize-in-place):
    /// updates existing charts using cached phenom_ids.
    /// Full chart fetch with all IDs stringified. Used by cmd_consolidate.
    pub fn fetch_all_with_ids(&self) -> Result<(Vec<Chart>, Vec<String>)> {
        match self {
            WebProvider::Luna { session, .. } => {
                let (charts, ids) = session
                    .fetch_charts(
                        None,
                        false,
                        &|i, total, name| {
                            print!("[{i:>3}/{total}] {name:<40}  ");
                            let _ = std::io::stdout().flush();
                        },
                        &|status| {
                            if !status.is_empty() {
                                println!("{status}");
                            }
                        },
                    )
                    .context("fetching LUNA charts")?;
                Ok((charts, ids))
            }
            WebProvider::Astrocom { session, .. } => {
                let (charts, ids) = session
                    .fetch_charts()
                    .context("fetching astro.com charts")?;
                Ok((charts, ids.into_iter().map(|n| n.to_string()).collect()))
            }
            WebProvider::Astrotheoros { session, .. } => {
                let (charts, uuids) = session
                    .fetch_charts()
                    .context("fetching astrotheoros.com charts")?;
                Ok((charts, uuids))
            }
        }
    }

    /// Delete a single chart by its stringified provider ID. Used by cmd_consolidate.
    ///
    /// Astrocom requires login credentials (not token-only) for deletion.
    pub fn delete_one(&self, id: &str) -> Result<()> {
        match self {
            WebProvider::Luna { session, .. } => {
                session.delete_phenom(id).context("deleting LUNA chart")?;
            }
            WebProvider::Astrocom { session, creds, .. } => {
                let (user, pass) = creds.as_ref().context(
                    "--consolidate --target astrocom requires --astrocom-user / --astrocom-pass \
                     (token-only sessions cannot delete charts)",
                )?;
                let nhor_id = id
                    .parse::<u32>()
                    .with_context(|| format!("invalid astrocom chart id: {id}"))?;
                session
                    .delete_charts(user, pass, &[nhor_id])
                    .context("deleting astro.com chart")?;
            }
            WebProvider::Astrotheoros { session, .. } => {
                session
                    .delete_one(id)
                    .context("deleting astrotheoros.com chart")?;
            }
        }
        Ok(())
    }

    /// Write charts to the web sink.  Returns per-chart statuses: `Some(msg)` for
    /// charts that were written (created or updated), `None` for pre-existing charts
    /// that were skipped.  Nothing is printed to stdout — callers use the returned
    /// statuses to compose output (e.g. inline with the readback transcript).
    /// Whether this provider verifies writes inline from the create response,
    /// with no separate readback.  True for astrotheoros (the `POST /api/chart`
    /// response echoes the full landed entry); false for providers whose create
    /// response does not, which fall back to a post-write readback.
    #[must_use]
    pub fn verifies_inline(&self) -> bool {
        matches!(self, WebProvider::Astrotheoros { .. })
    }

    /// Write charts to the web sink.
    ///
    /// For inline-verify providers (see [`Self::verifies_inline`]), `on_landed`
    /// is invoked the instant each chart lands — `(new_index, total_new, source,
    /// landed, status)` — so the caller can print a live per-chart block from the
    /// create response.  `landed` is `None` if the create failed.
    ///
    /// For other providers a transient single-line progress indicator is shown
    /// while writing (so the run is never silent), `on_landed` is not called, and
    /// the caller verifies afterward via a readback.
    ///
    /// Returns per-chart statuses: `Some(msg)` for charts that were written,
    /// `None` for pre-existing charts that were skipped.
    pub fn write_charts(
        &mut self,
        charts: &[Chart],
        on_landed: &mut LandedFn<'_>,
    ) -> Result<Vec<Option<String>>> {
        let tty = std::io::stdout().is_terminal();
        // Transient progress: redraw on the same line via CR + clear-to-EOL.
        let live_start = &|i: usize, total: usize, name: &str| {
            if tty {
                let w = total.to_string().len();
                print!("\r\x1b[Kwriting [{i:0>w$}/{total}] {name}");
                let _ = std::io::stdout().flush();
            }
        };
        // Store every status; print errors permanently (clearing the progress line first).
        let on_done = |results: &RefCell<Vec<String>>, s: &str| {
            if s.starts_with("[!]") {
                if tty {
                    print!("\r\x1b[K");
                }
                println!("{s}");
            }
            results.borrow_mut().push(s.to_string());
        };
        let mut statuses: Vec<Option<String>> = vec![None; charts.len()];

        match self {
            WebProvider::Luna {
                session,
                listing_keys,
                phenom_ids,
                ..
            } => {
                if phenom_ids.is_empty() {
                    // listing-keys mode: came from read_existing().
                    // Create only charts not already in LUNA.
                    let new_indices: Vec<usize> = charts
                        .iter()
                        .enumerate()
                        .filter(|(_, c)| !listing_keys.contains(&key(c)))
                        .map(|(i, _)| i)
                        .collect();
                    let skipped = charts.len() - new_indices.len();
                    if skipped > 0 {
                        println!("  {skipped} already in LUNA — skipped");
                    }
                    let new_charts: Vec<Chart> =
                        new_indices.iter().map(|&i| charts[i].clone()).collect();
                    let empty_ids = vec!["".to_string(); new_charts.len()];
                    let results: RefCell<Vec<String>> = RefCell::new(Vec::new());
                    session
                        .write_charts(&new_charts, &empty_ids, live_start, &|s: &str| {
                            on_done(&results, s)
                        })
                        .context("writing to LUNA")?;
                    for (idx, status) in new_indices.iter().zip(results.into_inner()) {
                        statuses[*idx] = Some(status);
                    }
                } else {
                    // phenom-ids mode: came from read_input() (normalize-in-place).
                    // Update existing charts using cached phenom_ids.
                    let results: RefCell<Vec<String>> = RefCell::new(Vec::new());
                    session
                        .write_charts(charts, phenom_ids, live_start, &|s: &str| {
                            on_done(&results, s)
                        })
                        .context("writing to LUNA")?;
                    for (i, status) in results.into_inner().into_iter().enumerate() {
                        statuses[i] = Some(status);
                    }
                }
            }
            WebProvider::Astrocom {
                session,
                nhor_id_map,
                ..
            } => {
                let ids: Vec<u32> = charts
                    .iter()
                    .map(|c| *nhor_id_map.get(&key(c)).unwrap_or(&0))
                    .collect();
                let new_indices: Vec<usize> = ids
                    .iter()
                    .enumerate()
                    .filter(|&(_, &id)| id == 0)
                    .map(|(i, _)| i)
                    .collect();
                let results: RefCell<Vec<String>> = RefCell::new(Vec::new());
                session
                    .write_charts(charts, &ids, live_start, &|s: &str| on_done(&results, s))
                    .context("writing to astro.com")?;
                for (idx, status) in new_indices.iter().zip(results.into_inner()) {
                    statuses[*idx] = Some(status);
                }
            }
            WebProvider::Astrotheoros {
                session, uuid_map, ..
            } => {
                let uuids: Vec<String> = charts
                    .iter()
                    .map(|c| uuid_map.get(&key(c)).cloned().unwrap_or_default())
                    .collect();
                // Inline verify: convert each create response to a landed Chart and
                // hand it to `on_landed` immediately; record the status by orig index.
                session
                    .write_charts(
                        charts,
                        &uuids,
                        &mut |orig_i, n, total, source, status, entry| {
                            let landed = entry.and_then(|e| entry_to_chart(e).ok());
                            on_landed(n, total, source, landed.as_ref(), status);
                            statuses[orig_i] = Some(status.to_string());
                        },
                    )
                    .context("writing to astrotheoros.com")?;
            }
        }
        // Clear the transient progress line so merged output starts clean.
        if tty {
            print!("\r\x1b[K");
            let _ = std::io::stdout().flush();
        }
        Ok(statuses)
    }
}
