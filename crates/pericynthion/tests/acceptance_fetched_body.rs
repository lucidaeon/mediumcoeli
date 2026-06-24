//! End-to-end: a Horizons-fetched Chiron appears in a computed chart.
//! Skips unless `$STARCAT_JPL_DATA` (DE441) and `$STARCAT_HORIZONS_DATA`
//! containing `20002060.bsp` are present. Fetch with `starcat horizons cent`.

use pericynthion::chart::{ChartRequest, ModeRequest, compute_with_spk};
use pericynthion::ephemeris::Ephemeris;
use pericynthion::jpl::{discover, header::parse, reader::EphemerisFile};
use pericynthion::spk::SpkEphemeris;
use pericynthion::time::calendar::{Calendar, CivilDate};
use pericynthion::time::zone::Zone;
use std::path::PathBuf;

/// Open the DE441 binary ephemeris from `$STARCAT_JPL_DATA`, if present.
fn open_de441() -> Option<(EphemerisFile, pericynthion::jpl::header::Header)> {
    let val = std::env::var_os("STARCAT_JPL_DATA")?;
    let dir = PathBuf::from(val);
    let loc = discover::locate(&dir).ok()?;
    let paths = match loc {
        discover::DatasetLocation::Binary(p) => p,
        discover::DatasetLocation::Ascii { .. } => return None,
    };
    let source = std::fs::read_to_string(&paths.header).ok()?;
    let header = parse(&source).ok()?;
    let file = EphemerisFile::open(&paths.binary, &header).ok()?;
    Some((file, header))
}

/// Resolve `$STARCAT_HORIZONS_DATA/20002060.bsp`, if present.
fn find_chiron() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("STARCAT_HORIZONS_DATA")?);
    let bsp = dir.join("20002060.bsp");
    bsp.is_file().then_some(bsp)
}

#[test]
fn chiron_appears_in_chart() {
    let (Some((file, header)), Some(chiron_bsp)) = (open_de441(), find_chiron()) else {
        eprintln!("skip: need $STARCAT_JPL_DATA (DE441) + $STARCAT_HORIZONS_DATA/20002060.bsp");
        return;
    };
    let ephem = Ephemeris::new(&file, &header).expect("build Ephemeris");
    let spk = SpkEphemeris::open(&chiron_bsp).expect("open 20002060.bsp");

    let req = ChartRequest {
        civil: CivilDate {
            year: 2000,
            month: 1,
            day: 1,
            hour: 12,
            minute: 0,
            second: 0.0,
        },
        calendar: Calendar::Gregorian,
        zone: Zone::FixedSeconds(0),
        mode: ModeRequest::Geocentric,
        lat_deg: None,
        lon_deg: None,
        bodies: None,
        houses: Vec::new(),
        asteroids: vec![20_002_060],
    };

    let chart = compute_with_spk(&ephem, &[&spk], &req, &[]).expect("compute_with_spk");
    let chiron = chart
        .asteroids
        .iter()
        .find(|a| a.naif_id == 20_002_060)
        .expect("Chiron present in chart");
    assert_eq!(chiron.name, "Chiron");
    let lon = chiron.position.longitude_deg;
    assert!(
        lon.is_finite() && (0.0..360.0).contains(&lon),
        "Chiron longitude out of [0,360): {lon}"
    );
    eprintln!("Chiron @ 2000-01-01 12:00 UT: {lon:.4}° ecliptic");
}
