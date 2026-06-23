//! Live integration tests: write a synthetic chart then delete it from each web target.
//!
//! Each test checks for the required env vars and skips without failing if absent,
//! so `cargo test` stays green in environments without credentials.
//!
//! To run (tests are `#[ignore]` — opt-in only):
//!   LUNA_TOKEN=<cookie>               cargo test --test web_integration -- --ignored luna
//!   ASTROCOM_USER=<email> ASTROCOM_PASS=<pass>  cargo test --test web_integration -- --ignored astrocom
//!   ASTROTHEOROS_USER=<u> ASTROTHEOROS_PASS=<p> cargo test --test web_integration -- --ignored astrotheoros

use astrogram::chart::{
    Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, Zodiac,
};

/// Generate a chart from random parameters on every call.
///
/// Parameters:
/// - Year: 1658–2024
/// - Month/day: any calendar-valid combination (day capped at 28)
/// - Time of day: any hour/minute
/// - Location: one of 41 world cities >10M population (lat/lon from the fixed table below)
/// - Name: two words drawn from a palette of color/nature names
///
/// Seeds from wall-clock nanoseconds so every test run is different.
fn synthetic_chart() -> Chart {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos() as u64;
    let mut s = seed
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    let mut rng = || -> u64 {
        s = s
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        s >> 33
    };

    const NAMES: &[&str] = &[
        "Amber",
        "Auburn",
        "Azure",
        "Blush",
        "Celeste",
        "Cerise",
        "Charcoal",
        "Cherry",
        "Cinnamon",
        "Claret",
        "Cocoa",
        "Coral",
        "Crimson",
        "Cyan",
        "Dove",
        "Ebony",
        "Emerald",
        "Flora",
        "Fuchsia",
        "Ginger",
        "Hazel",
        "Heather",
        "Heliotrope",
        "Honey",
        "Indigo",
        "Iris",
        "Ivory",
        "Jade",
        "Lavender",
        "Lemon",
        "Lilac",
        "Magenta",
        "Marigold",
        "Mauve",
        "Mint",
        "Navy",
        "Olive",
        "Opal",
        "Peach",
        "Pearl",
        "Penny",
        "Periwinkle",
        "Plum",
        "Primrose",
        "Raven",
        "Rose",
        "Ruby",
        "Russet",
        "Saffron",
        "Sapphire",
        "Scarlett",
        "Sepia",
        "Sienna",
        "Silver",
        "Sky",
        "Tawny",
        "Teal",
        "Veridian",
        "Violet",
        "Willow",
        "Wisteria",
    ];

    // World cities with >10M population. Mirrors skills/astrologer/fixtures/ref_synthetics.md (city,lat,long).
    // lat/lon used directly by Luna and Astrotheoros; astro.com resolves the name via atlas.
    const CITIES: &[(&str, f64, f64)] = &[
        ("Bangkok", 13.76, 100.50),
        ("Beijing", 39.90, 116.41),
        ("Bengaluru", 12.97, 77.59),
        ("Bogotá", 4.71, -74.07),
        ("Buenos Aires", -34.60, -58.38),
        ("Cairo", 30.04, 31.24),
        ("Chennai", 13.08, 80.27),
        ("Chongqing", 29.56, 106.55),
        ("Dhaka", 23.81, 90.41),
        ("Delhi", 28.61, 77.21),
        ("Guangzhou", 23.13, 113.26),
        ("Hangzhou", 30.27, 120.15),
        ("Ho Chi Minh City", 10.82, 106.63),
        ("Hyderabad", 17.39, 78.49),
        ("Istanbul", 41.01, 28.98),
        ("Jakarta", -6.21, 106.85),
        ("Karachi", 24.86, 67.01),
        ("Kinshasa", -4.32, 15.31),
        ("Kolkata", 22.57, 88.36),
        ("Lagos", 6.52, 3.38),
        ("Lahore", 31.55, 74.34),
        ("Lima", -12.05, -77.04),
        ("London", 51.51, -0.13),
        ("Los Angeles", 34.05, -118.24),
        ("Luanda", -8.84, 13.23),
        ("Manila", 14.60, 120.98),
        ("Mexico City", 19.43, -99.13),
        ("Moscow", 55.76, 37.62),
        ("Mumbai", 19.08, 72.88),
        ("New York City", 40.71, -74.01),
        ("Osaka", 34.69, 135.50),
        ("Paris", 48.85, 2.35),
        ("Rio de Janeiro", -22.91, -43.17),
        ("São Paulo", -23.55, -46.63),
        ("Seoul", 37.57, 126.98),
        ("Shanghai", 31.23, 121.47),
        ("Shenzhen", 22.54, 114.06),
        ("Tehran", 35.69, 51.39),
        ("Tianjin", 39.34, 117.36),
        ("Tokyo", 35.69, 139.69),
        ("Wuhan", 30.59, 114.31),
    ];

    let first = NAMES[(rng() as usize) % NAMES.len()];
    let last = NAMES[(rng() as usize) % NAMES.len()];
    let (city, lat, lon) = CITIES[(rng() as usize) % CITIES.len()];
    let year = 1658i16 + (rng() % (2024 - 1658 + 1)) as i16;
    let month = 1u8 + (rng() % 12) as u8;
    let day = 1u8 + (rng() % 28) as u8; // 1–28 valid for every month
    let hour = (rng() % 24) as u8;
    let minute = (rng() % 60) as u8;
    let tz = (lon / 15.0).round().clamp(-12.0, 14.0);

    Chart {
        name: format!("{first} {last}"),
        secondary_name: None,
        city: Some(city.to_string()),
        region: None,
        longitude: Longitude::new(lon).unwrap(),
        latitude: Latitude::new(lat).unwrap(),
        year,
        month,
        day,
        hour,
        minute,
        second: 0,
        tz_offset_hours: tz,
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

// ── LUNA® ─────────────────────────────────────────────────────────────────────

#[test]
#[ignore = "live network — run with: LUNA_TOKEN=<cookie> cargo test --test web_integration -- --ignored luna"]
fn luna_write_delete_synthetic() {
    let cookie = match std::env::var("LUNA_TOKEN") {
        Ok(c) => c,
        Err(_) => {
            println!("LUNA_TOKEN not set — skipping");
            return;
        }
    };

    let chart = synthetic_chart();
    println!(
        "LUNA: chart=\"{}\" {}-{:02}-{:02} {:02}:{:02} city={}",
        chart.name,
        chart.year,
        chart.month,
        chart.day,
        chart.hour,
        chart.minute,
        chart.city.as_deref().unwrap_or("")
    );

    let session = astrogram::luna::LunaSession::new(&cookie, 500).expect("LunaSession::new");

    let phenom_id = session
        .create_one(&chart)
        .expect("create synthetic chart on LUNA");
    assert!(!phenom_id.is_empty(), "expected a non-empty phenom_id");
    println!("LUNA: created phenom_id={phenom_id}");

    // Delete by captured phenom_id; fall back to searching by name.
    if session.delete_phenom(&phenom_id).is_err() {
        println!("LUNA: direct delete failed — searching by name for cleanup");
        let (charts, ids) = session
            .fetch_charts(None, false, &|_, _, _| {}, &|_| {})
            .expect("fetch_charts for cleanup");
        let idx = charts
            .iter()
            .position(|c| c.name == chart.name)
            .expect("chart not found by name after failed delete");
        session
            .delete_phenom(&ids[idx])
            .expect("fallback delete by name");
        println!("LUNA: deleted by name (fallback)");
    } else {
        println!("LUNA: deleted phenom_id={phenom_id}");
    }
}

// ── astro.com ─────────────────────────────────────────────────────────────────

#[test]
#[ignore = "live network — run with: ASTROCOM_USER=<email> ASTROCOM_PASS=<pass> cargo test --test web_integration -- --ignored astrocom"]
fn astrocom_write_delete_synthetic() {
    let user = match std::env::var("ASTROCOM_USER") {
        Ok(u) => u,
        Err(_) => {
            println!("ASTROCOM_USER not set — skipping");
            return;
        }
    };
    let pass = match std::env::var("ASTROCOM_PASS") {
        Ok(p) => p,
        Err(_) => {
            println!("ASTROCOM_PASS not set — skipping");
            return;
        }
    };

    let chart = synthetic_chart();
    println!(
        "astro.com: chart=\"{}\" {}-{:02}-{:02} {:02}:{:02} city={}",
        chart.name,
        chart.year,
        chart.month,
        chart.day,
        chart.hour,
        chart.minute,
        chart.city.as_deref().unwrap_or("")
    );

    let session = astrogram::astrocom::AstrocomSession::login(&user, &pass, 500)
        .expect("AstrocomSession::login");

    let nhor_id = session
        .create_one(&chart)
        .expect("create synthetic chart on astro.com");
    assert!(nhor_id > 0, "expected a non-zero nhor_id");
    println!("astro.com: created nhor_id={nhor_id}");

    // Delete by captured nhor_id; fall back to searching by name.
    if session.delete_charts(&user, &pass, &[nhor_id]).is_err() {
        println!("astro.com: direct delete failed — searching by name for cleanup");
        let (charts, ids) = session.fetch_charts().expect("fetch_charts for cleanup");
        let idx = charts
            .iter()
            .position(|c| c.name == chart.name)
            .expect("chart not found by name after failed delete");
        session
            .delete_charts(&user, &pass, &[ids[idx]])
            .expect("fallback delete by name");
        println!("astro.com: deleted by name (fallback)");
    } else {
        println!("astro.com: deleted nhor_id={nhor_id}");
    }
}

// ── astrotheoros.com ──────────────────────────────────────────────────────────

#[test]
#[ignore = "live network — run with: ASTROTHEOROS_USER=<u> ASTROTHEOROS_PASS=<p> cargo test --test web_integration -- --ignored astrotheoros"]
fn astrotheoros_write_delete_synthetic() {
    let user = match std::env::var("ASTROTHEOROS_USER") {
        Ok(u) => u,
        Err(_) => {
            println!("ASTROTHEOROS_USER not set — skipping");
            return;
        }
    };
    let pass = match std::env::var("ASTROTHEOROS_PASS") {
        Ok(p) => p,
        Err(_) => {
            println!("ASTROTHEOROS_PASS not set — skipping");
            return;
        }
    };

    let chart = synthetic_chart();
    println!(
        "astrotheoros.com: chart=\"{}\" {}-{:02}-{:02} {:02}:{:02} city={}",
        chart.name,
        chart.year,
        chart.month,
        chart.day,
        chart.hour,
        chart.minute,
        chart.city.as_deref().unwrap_or("")
    );

    let session = astrogram::astrotheoros::AstrotheorosSession::login(&user, &pass, 500)
        .expect("AstrotheorosSession::login");

    let entry = session
        .create_one(&chart)
        .expect("create synthetic chart on astrotheoros.com");
    let uuid = entry.id;
    assert!(!uuid.is_empty(), "expected a non-empty UUID");
    println!("astrotheoros.com: created uuid={uuid}");

    // Delete by captured UUID; fall back to searching by name.
    if session.delete_one(&uuid).is_err() {
        println!("astrotheoros.com: direct delete failed — searching by name for cleanup");
        let (charts, uuids) = session.fetch_charts().expect("fetch_charts for cleanup");
        let idx = charts
            .iter()
            .position(|c| c.name == chart.name)
            .expect("chart not found by name after failed delete");
        session
            .delete_one(&uuids[idx])
            .expect("fallback delete by name");
        println!("astrotheoros.com: deleted by name (fallback)");
    } else {
        println!("astrotheoros.com: deleted uuid={uuid}");
    }
}
