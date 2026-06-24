//! Acceptance tests for fixed star tropical longitude drift.
//!
//!   cargo test --release -p pericynthion --test fixed_stars_drift -- --nocapture
//!
//! When $ASTRO_RESEARCH is set, the century_table test writes
//! fixed_stars_drift.md there.
use pericynthion::stars::{CATALOG, compute_star, galactic_center};

const J2000: f64 = 2_451_545.0;
const JULIAN_YEAR_DAYS: f64 = 365.25;

fn jd_for_year(year_ce: i32) -> f64 {
    J2000 + (f64::from(year_ce) - 2000.0) * JULIAN_YEAR_DAYS
}

#[test]
fn galactic_center_drift_rate_is_approx_1_4_deg_per_century() {
    let jd1900 = J2000 - 100.0 * JULIAN_YEAR_DAYS;
    let jd2000 = J2000;
    let jd2100 = J2000 + 100.0 * JULIAN_YEAR_DAYS;

    let lon1900 = galactic_center(jd1900).longitude_deg;
    let lon2000 = galactic_center(jd2000).longitude_deg;
    let lon2100 = galactic_center(jd2100).longitude_deg;

    let drift_past = lon2000 - lon1900;
    let drift_future = lon2100 - lon2000;

    eprintln!("GC J1900: {lon1900:.4}°  J2000: {lon2000:.4}°  J2100: {lon2100:.4}°");
    eprintln!("Drift 1900→2000: {drift_past:.4}°/century");
    eprintln!("Drift 2000→2100: {drift_future:.4}°/century");

    // General precession in longitude at J2000: ~5029.097″/century = 1.3970°/century.
    // Allow ±0.1° for higher-order terms.
    assert!(
        (1.3..1.5).contains(&drift_past),
        "Past drift {drift_past:.4}°/century not in [1.3, 1.5]"
    );
    assert!(
        (1.3..1.5).contains(&drift_future),
        "Future drift {drift_future:.4}°/century not in [1.3, 1.5]"
    );
}

#[test]
fn all_stars_j2000_snapshot() {
    eprintln!(
        "\n{:<16}  {:>8}  {:>16}  {:>8}",
        "Star", "Lon (°)", "Sign position", "Lat (°)"
    );
    eprintln!("{}", "-".repeat(56));
    for star in &CATALOG {
        let pos = compute_star(star, J2000);
        let sign_deg = pos.longitude_deg % 30.0;
        let sign = sign_name(pos.longitude_deg);
        eprintln!(
            "{:<16}  {:>8.3}  {:>5.2}° {:<9}  {:>8.3}",
            star.name, pos.longitude_deg, sign_deg, sign, pos.latitude_deg
        );
        assert!((0.0..360.0).contains(&pos.longitude_deg));
    }
}

#[test]
fn fixed_stars_century_table() {
    // GC + four Royal Stars at century epochs 0–2200 CE.
    let royal_indices = [2usize, 7, 10, 11]; // Aldebaran, Regulus, Antares, Fomalhaut
    let years: Vec<i32> = (0..=2200).step_by(100).collect();

    eprintln!("\n# Fixed star tropical longitudes by century (Yale BSC5P, IAU 2006 precession)\n");
    eprintln!(
        "| {:>7} | {:>8} | {:>8} | {:>8} | {:>8} | {:>8} |",
        "Year", "GC (°)", "Aldeb.", "Regulus", "Antares", "Fomalh."
    );
    eprintln!(
        "|{}|{}|{}|{}|{}|{}|",
        "-".repeat(9),
        "-".repeat(10),
        "-".repeat(10),
        "-".repeat(10),
        "-".repeat(10),
        "-".repeat(10)
    );

    let mut rows: Vec<(i32, f64, [f64; 4])> = Vec::new();
    for &year in &years {
        let jd = jd_for_year(year);
        let gc = galactic_center(jd).longitude_deg;
        let royal: [f64; 4] = royal_indices.map(|i| compute_star(&CATALOG[i], jd).longitude_deg);
        eprintln!(
            "| {:>7} | {:>8.3} | {:>8.3} | {:>8.3} | {:>8.3} | {:>8.3} |",
            year, gc, royal[0], royal[1], royal[2], royal[3]
        );
        rows.push((year, gc, royal));
    }

    let Ok(research_dir) = std::env::var("ASTRO_RESEARCH") else {
        eprintln!("\n($ASTRO_RESEARCH unset — not writing file)");
        return;
    };
    let out_path = std::path::Path::new(&research_dir).join("fixed_stars_drift.md");
    let mut md = String::from(
        "# Fixed star tropical longitudes by century\n\n\
         Source: `pericynthion::stars`, IAU 2006 precession + IAU 2000B nutation.\n\
         Star coordinates: Yale BSC5P (Hoffleit & Warren 1991) J2000 ICRS.\n\
         GC (Sgr A*) coordinates: Reid & Brunthaler 2004.\n\
         Proper motion not applied.\n\
         Royal Stars: Aldebaran, Regulus, Antares, Fomalhaut. GC = Galactic Center.\n\n\
         | Year CE | GC (°) | Aldebaran | Regulus | Antares | Fomalhaut |\n\
         |---------|--------|-----------|---------|---------|----------|\n",
    );
    for (year, gc, royal) in rows {
        md.push_str(&format!(
            "| {:>7} | {:>6.2} | {:>9.2} | {:>7.2} | {:>7.2} | {:>8.2} |\n",
            year, gc, royal[0], royal[1], royal[2], royal[3]
        ));
    }
    std::fs::write(&out_path, md).expect("write fixed_stars_drift.md");
    eprintln!("\nWrote: {}", out_path.display());
}

fn sign_name(lon_deg: f64) -> &'static str {
    let idx = (lon_deg / 30.0) as usize % 12;
    [
        "Aries",
        "Taurus",
        "Gemini",
        "Cancer",
        "Leo",
        "Virgo",
        "Libra",
        "Scorpio",
        "Sagittarius",
        "Capricorn",
        "Aquarius",
        "Pisces",
    ][idx]
}
