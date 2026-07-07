//! Provider adapter over the `luna` / `astrocom` / `astrotheoros` web modules.
//!
//! Terminal output is decoupled behind [`ProgressSink`] (no-op by default), so
//! the CLI supplies a printing sink and a GUI supplies its own or none. This is
//! the full web-CRUD lifecycle a frontend drives: read existing → read input →
//! write → verify → delete.

use crate::astrocom::AstrocomSession;
use crate::astrotheoros::{AstrotheorosSession, entry_to_chart};
use crate::capability::ChartField;
use crate::chart::{Chart, CoordinateSystem, HouseSystem, Zodiac};
use crate::luna::LunaSession;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

/// Full-identity dedup key: name plus birth datetime. Distinct from
/// [`crate::transcript::temporal_key`] (datetime only, used for readback
/// pairing of possibly-renamed charts).
pub type DatetimeKey = (String, i16, u8, u8, u8, u8, u8);

/// Per-record callback for `WebProvider::write_charts` on inline-verify
/// providers: `(new_index, total_new, source, landed, status)`. `landed` is
/// `None` if the create failed.
pub type LandedFn<'a> = dyn FnMut(usize, usize, &Chart, Option<&Chart>, &str) + 'a;

/// The full-identity key for a chart.
#[must_use]
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
/// diffing. `field_notes` carries provenance notes (e.g. `"global setting"`)
/// for the transcript.
pub struct GlobalRender {
    /// Global house system the sink renders with.
    pub house_system: HouseSystem,
    /// Global zodiac the sink renders with.
    pub zodiac: Zodiac,
    /// Coordinate system the sink renders with.
    pub coordinate_system: CoordinateSystem,
    /// Per-field provenance notes for the transcript.
    pub field_notes: Vec<(ChartField, &'static str)>,
}

impl GlobalRender {
    /// Fold these account-wide settings into `chart` (web providers store house
    /// system / zodiac / coordinate system per account, not per chart).
    ///
    /// This method must be extended whenever [`crate::capability::NON_OMITTABLE`]
    /// gains a new field; the `apply_to_sets_every_non_omittable_field` pin test
    /// enforces that all three current fields are assigned.
    pub fn apply_to(&self, chart: &mut Chart) {
        chart.house_system = self.house_system;
        chart.zodiac = self.zodiac;
        chart.coordinate_system = self.coordinate_system;
    }
}

/// Progress/disclosure sink for provider operations. Every method has a no-op
/// default, so a GUI can ignore progress entirely and the CLI overrides only
/// what it prints. All methods take `&self` so they thread into the session
/// modules' `&dyn Fn` progress closures without interior mutability.
pub trait ProgressSink {
    /// A phase announcement, e.g. `"LUNA (existing): reading…"`.
    fn phase(&self, _msg: &str) {}
    /// A count line, e.g. `"Found 12 charts in LUNA"`.
    fn count(&self, _msg: &str) {}
    /// A per-item start, e.g. read/fetch row `[i/total] name`.
    fn item_start(&self, _i: usize, _total: usize, _name: &str) {}
    /// A per-item result/status string following an `item_start`.
    fn item_result(&self, _status: &str) {}
    /// Transient write progress `[i/total] name` (CLI redraws in place).
    fn write_progress(&self, _i: usize, _total: usize, _name: &str) {}
    /// A write error line for a single record.
    fn write_error(&self, _msg: &str) {}
    /// A neutral note, e.g. `"3 already in LUNA — skipped"`.
    fn note(&self, _msg: &str) {}
    /// Clear any transient progress line after a write run completes.
    fn write_done(&self) {}
}

/// Errors from the provider layer. Wraps each web module's error.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    /// A LUNA operation failed.
    #[error(transparent)]
    Luna(#[from] crate::luna::LunaError),
    /// An astro.com operation failed.
    #[error(transparent)]
    Astrocom(#[from] crate::astrocom::AstrocomError),
    /// An astrotheoros.com operation failed.
    #[error(transparent)]
    Astrotheoros(#[from] crate::astrotheoros::AstrotheorosError),
    /// A provider-layer precondition failed (e.g. delete without credentials).
    #[error("{0}")]
    Other(String),
}

/// Adapter over the three web backends (`luna`, `astrocom`, `astrotheoros`).
///
/// Encapsulates the per-provider session, per-run state (ID caches), and
/// configuration flags. Each method maps uniformly to the underlying session
/// API and routes progress messages through a [`ProgressSink`].
pub enum WebProvider {
    /// LUNA astrology platform.
    Luna {
        /// Active session.
        session: LunaSession,
        /// Stored at construction from `cli.luna_resume_from`; used by `read_input`.
        resume_from: Option<String>,
        /// Stored at construction from `cli.normalize`; used by `read_input`.
        normalize: bool,
        /// Populated by `read_existing`: listing keys for create-only dedup.
        listing_keys: HashSet<DatetimeKey>,
        /// Populated by `read_input`: phenom ids for update-in-place.
        phenom_ids: Vec<String>,
    },
    /// astro.com platform.
    Astrocom {
        /// Active session.
        session: AstrocomSession,
        /// Populated when login path was used; needed by `delete_one`.
        /// `None` when token-only — `delete_one` returns an error in that case.
        creds: Option<(String, String)>,
        /// Populated by `read_existing` or `read_input`.
        nhor_id_map: HashMap<DatetimeKey, u32>,
    },
    /// astrotheoros.com platform.
    Astrotheoros {
        /// Active session.
        session: AstrotheorosSession,
        /// Populated by `read_existing` or `read_input`.
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
    /// Luna: fetches listing keys only (no per-chart HTTP); returns `[]`.
    /// Astrocom / Astrotheoros: fetches all charts; returns them.
    ///
    /// # Errors
    ///
    /// Returns `ProviderError` if the underlying session call fails.
    pub fn read_existing(&mut self, sink: &dyn ProgressSink) -> Result<Vec<Chart>, ProviderError> {
        match self {
            WebProvider::Luna {
                session,
                listing_keys,
                ..
            } => {
                sink.phase("LUNA (existing): reading\u{2026}");
                let rows = session.fetch_listing()?;
                sink.count(&format!("Found {} charts in LUNA", rows.len()));
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
                sink.phase("astro.com (existing): reading\u{2026}");
                let (charts, ids) = session.fetch_charts()?;
                sink.count(&format!("astro.com: {} charts (existing)", charts.len()));
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
                sink.phase("astrotheoros.com (existing): reading\u{2026}");
                let (charts, uuids) = session.fetch_charts()?;
                sink.count(&format!(
                    "astrotheoros.com: {} charts (existing)",
                    charts.len()
                ));
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
    ///
    /// # Errors
    ///
    /// Returns `ProviderError` if the underlying session call fails.
    pub fn read_input(&mut self, sink: &dyn ProgressSink) -> Result<Vec<Chart>, ProviderError> {
        match self {
            WebProvider::Luna {
                session,
                resume_from,
                normalize,
                phenom_ids,
                ..
            } => {
                let (charts, ids) = session.fetch_charts(
                    resume_from.as_deref(),
                    *normalize,
                    &|i, total, name| {
                        sink.item_start(i, total, name);
                    },
                    &|status| {
                        if !status.is_empty() {
                            sink.item_result(status);
                        }
                    },
                )?;
                *phenom_ids = ids;
                Ok(charts)
            }
            WebProvider::Astrocom {
                session,
                nhor_id_map,
                ..
            } => {
                let (charts, ids) = session.fetch_charts()?;
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
                let (charts, uuids) = session.fetch_charts()?;
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
    ///
    /// # Errors
    ///
    /// Returns `ProviderError` if the astrotheoros settings fetch fails.
    pub fn fetch_global_settings(&self) -> Result<Option<GlobalRender>, ProviderError> {
        match self {
            WebProvider::Astrotheoros { session, .. } => {
                let s = session.fetch_settings()?;
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

    /// Full chart fetch with all IDs stringified. Used by `cmd_consolidate`.
    ///
    /// # Errors
    ///
    /// Returns `ProviderError` if the underlying session call fails.
    pub fn fetch_all_with_ids(
        &self,
        sink: &dyn ProgressSink,
    ) -> Result<(Vec<Chart>, Vec<String>), ProviderError> {
        match self {
            WebProvider::Luna { session, .. } => {
                let (charts, ids) = session.fetch_charts(
                    None,
                    false,
                    &|i, total, name| {
                        sink.item_start(i, total, name);
                    },
                    &|status| {
                        if !status.is_empty() {
                            sink.item_result(status);
                        }
                    },
                )?;
                Ok((charts, ids))
            }
            WebProvider::Astrocom { session, .. } => {
                let (charts, ids) = session.fetch_charts()?;
                Ok((charts, ids.into_iter().map(|n| n.to_string()).collect()))
            }
            WebProvider::Astrotheoros { session, .. } => {
                let (charts, uuids) = session.fetch_charts()?;
                Ok((charts, uuids))
            }
        }
    }

    /// Delete a single chart by its stringified provider ID. Used by `cmd_consolidate`.
    ///
    /// Astrocom requires login credentials (not token-only) for deletion.
    ///
    /// # Errors
    ///
    /// Returns `ProviderError::Other` if astrocom credentials are absent or the
    /// id is not a valid `u32`. Returns the session error variant for network failures.
    pub fn delete_one(&self, id: &str) -> Result<(), ProviderError> {
        match self {
            WebProvider::Luna { session, .. } => {
                session.delete_phenom(id)?;
            }
            WebProvider::Astrocom { session, creds, .. } => {
                let (user, pass) = creds.as_ref().ok_or_else(|| {
                    ProviderError::Other(
                        "--consolidate --target astrocom requires --astrocom-user / --astrocom-pass \
                         (token-only sessions cannot delete charts)"
                            .to_string(),
                    )
                })?;
                let nhor_id = id.parse::<u32>().map_err(|_| {
                    ProviderError::Other(format!("invalid astrocom chart id: {id}"))
                })?;
                session.delete_charts(user, pass, &[nhor_id])?;
            }
            WebProvider::Astrotheoros { session, .. } => {
                session.delete_one(id)?;
            }
        }
        Ok(())
    }

    /// Whether this provider verifies writes inline from the create response,
    /// with no separate readback. True for astrotheoros (the `POST /api/chart`
    /// response echoes the full landed entry); false for providers whose create
    /// response does not, which fall back to a post-write readback.
    #[must_use]
    pub fn verifies_inline(&self) -> bool {
        matches!(self, WebProvider::Astrotheoros { .. })
    }

    /// Human-readable name for the site, for progress and error messages.
    #[must_use]
    pub fn site_display(&self) -> &'static str {
        match self {
            WebProvider::Luna { .. } => "LUNA",
            WebProvider::Astrocom { .. } => "astro.com",
            WebProvider::Astrotheoros { .. } => "astrotheoros.com",
        }
    }

    /// Write charts to the web sink.
    ///
    /// For inline-verify providers (see [`Self::verifies_inline`]), `on_landed`
    /// is invoked the instant each chart lands — `(new_index, total_new, source,
    /// landed, status)` — so the caller can print a live per-chart block from the
    /// create response. `landed` is `None` if the create failed.
    ///
    /// For other providers, `sink.write_progress` is called while writing so
    /// the run is never silent; `on_landed` is not called, and the caller
    /// verifies afterward via a readback.
    ///
    /// Returns per-chart statuses: `Some(msg)` for charts that were written,
    /// `None` for pre-existing charts that were skipped.
    ///
    /// # Errors
    ///
    /// Returns `ProviderError` if any underlying session write call fails.
    pub fn write_charts(
        &mut self,
        charts: &[Chart],
        sink: &dyn ProgressSink,
        on_landed: &mut LandedFn<'_>,
    ) -> Result<Vec<Option<String>>, ProviderError> {
        let mut statuses: Vec<Option<String>> = vec![None; charts.len()];

        match self {
            WebProvider::Luna {
                session,
                listing_keys,
                phenom_ids,
                ..
            } => {
                write_charts_luna(
                    session,
                    listing_keys,
                    phenom_ids,
                    charts,
                    sink,
                    &mut statuses,
                )?;
            }
            WebProvider::Astrocom {
                session,
                nhor_id_map,
                ..
            } => {
                write_charts_astrocom(session, nhor_id_map, charts, sink, &mut statuses)?;
            }
            WebProvider::Astrotheoros {
                session, uuid_map, ..
            } => {
                write_charts_astrotheoros(session, uuid_map, charts, on_landed, &mut statuses)?;
            }
        }
        sink.write_done();
        Ok(statuses)
    }
}

/// Pushes a write-result status string into the shared results accumulator and routes `[!]`-prefixed error statuses to the progress sink.
fn on_write_result(results: &RefCell<Vec<String>>, sink: &dyn ProgressSink, s: &str) {
    if s.starts_with("[!]") {
        sink.write_error(s);
    }
    results.borrow_mut().push(s.to_string());
}

fn write_charts_luna(
    session: &LunaSession,
    listing_keys: &HashSet<DatetimeKey>,
    phenom_ids: &[String],
    charts: &[Chart],
    sink: &dyn ProgressSink,
    statuses: &mut [Option<String>],
) -> Result<(), ProviderError> {
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
            sink.note(&format!("  {skipped} already in LUNA \u{2014} skipped"));
        }
        let new_charts: Vec<Chart> = new_indices.iter().map(|&i| charts[i].clone()).collect();
        let empty_ids = vec![String::new(); new_charts.len()];
        let results: RefCell<Vec<String>> = RefCell::new(Vec::new());
        session.write_charts(
            &new_charts,
            &empty_ids,
            &|i, total, name| sink.write_progress(i, total, name),
            &|s: &str| on_write_result(&results, sink, s),
        )?;
        for (idx, status) in new_indices.iter().zip(results.into_inner()) {
            statuses[*idx] = Some(status);
        }
    } else {
        // phenom-ids mode: came from read_input() (normalize-in-place).
        // Update existing charts using cached phenom_ids.
        let results: RefCell<Vec<String>> = RefCell::new(Vec::new());
        session.write_charts(
            charts,
            phenom_ids,
            &|i, total, name| sink.write_progress(i, total, name),
            &|s: &str| on_write_result(&results, sink, s),
        )?;
        for (i, status) in results.into_inner().into_iter().enumerate() {
            statuses[i] = Some(status);
        }
    }
    Ok(())
}

fn write_charts_astrocom(
    session: &AstrocomSession,
    nhor_id_map: &HashMap<DatetimeKey, u32>,
    charts: &[Chart],
    sink: &dyn ProgressSink,
    statuses: &mut [Option<String>],
) -> Result<(), ProviderError> {
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
    session.write_charts(
        charts,
        &ids,
        &|i, total, name| sink.write_progress(i, total, name),
        &|s: &str| on_write_result(&results, sink, s),
    )?;
    for (idx, status) in new_indices.iter().zip(results.into_inner()) {
        statuses[*idx] = Some(status);
    }
    Ok(())
}

fn write_charts_astrotheoros(
    session: &AstrotheorosSession,
    uuid_map: &HashMap<DatetimeKey, String>,
    charts: &[Chart],
    on_landed: &mut LandedFn<'_>,
    statuses: &mut [Option<String>],
) -> Result<(), ProviderError> {
    let uuids: Vec<String> = charts
        .iter()
        .map(|c| uuid_map.get(&key(c)).cloned().unwrap_or_default())
        .collect();
    // Inline verify: convert each create response to a landed Chart and
    // hand it to `on_landed` immediately; record the status by orig index.
    session.write_charts(
        charts,
        &uuids,
        &mut |orig_i, n, total, source, status, entry| {
            let landed = entry.and_then(|e| entry_to_chart(e).ok());
            on_landed(n, total, source, landed.as_ref(), status);
            statuses[orig_i] = Some(status.to_string());
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_carries_name_and_datetime() {
        let c = crate::test_support::fully_populated();
        let k = key(&c);
        assert_eq!(k.0, "Anna Freud");
        assert_eq!(k.1, 1895);
        assert_eq!(k.2, 12);
        assert_eq!(k.3, 3);
    }

    #[test]
    fn noop_sink_default_methods_compile() {
        struct Noop;
        impl ProgressSink for Noop {}
        let s = Noop;
        s.phase("x");
        s.count("y");
        s.item_start(1, 2, "n");
        s.item_result("ok");
        s.write_progress(1, 2, "n");
        s.write_error("e");
        s.note("z");
    }

    /// Pin: `apply_to` must assign every `NON_OMITTABLE` field. If a fourth field
    /// is added to `NON_OMITTABLE` without updating `apply_to`, this test fails.
    #[test]
    fn apply_to_sets_every_non_omittable_field() {
        use crate::capability::NON_OMITTABLE;
        use crate::chart::{CoordinateSystem, HouseSystem, Zodiac};

        let mut chart = crate::test_support::fully_populated();
        // Start with values distinct from what GlobalRender will apply.
        chart.house_system = HouseSystem::Placidus;
        chart.zodiac = Zodiac::Tropical;
        chart.coordinate_system = CoordinateSystem::Geocentric;

        let global = GlobalRender {
            house_system: HouseSystem::Koch,
            zodiac: Zodiac::Lahiri,
            coordinate_system: CoordinateSystem::Heliocentric,
            field_notes: vec![],
        };
        global.apply_to(&mut chart);

        // Verify every NON_OMITTABLE field changed.
        assert_eq!(
            chart.house_system,
            HouseSystem::Koch,
            "apply_to must set house_system"
        );
        assert_eq!(chart.zodiac, Zodiac::Lahiri, "apply_to must set zodiac");
        assert_eq!(
            chart.coordinate_system,
            CoordinateSystem::Heliocentric,
            "apply_to must set coordinate_system"
        );
        // Belt: count matches so a new NON_OMITTABLE field fails here.
        assert_eq!(
            NON_OMITTABLE.len(),
            3,
            "NON_OMITTABLE gained a field — extend GlobalRender::apply_to and this test"
        );
    }

    #[test]
    fn provider_error_converts_from_session_errors() {
        // Compile-time proof the `?`/From wiring exists for each variant.
        fn _accepts_luna(e: crate::luna::LunaError) -> ProviderError {
            ProviderError::from(e)
        }
        fn _accepts_astrocom(e: crate::astrocom::AstrocomError) -> ProviderError {
            ProviderError::from(e)
        }
        fn _accepts_astrotheoros(e: crate::astrotheoros::AstrotheorosError) -> ProviderError {
            ProviderError::from(e)
        }
        // verifies_inline is a pure predicate on the variant tag — exercised via
        // a constructed Astrotheoros variant is not possible without a session,
        // so this test only locks the From conversions; behavior parity for the
        // session-driven methods is covered by blackmoon's network-gated tests.
        let _ = (_accepts_luna, _accepts_astrocom, _accepts_astrotheoros);
    }
}
