//! Validates emitted JZOD against the published JSON Schema and round-trips it.

use jzod::{
    Angle, AngleId, Body, BodyId, Chart, ChartType, Component, CoordinateSystem, DataSource,
    Datetime, Ephemeris, Generator, JzodDocument, Location, Placements, Position, Sect, Zodiac,
};
use std::collections::BTreeMap;

fn full_chart() -> Chart {
    let mut house = BTreeMap::new();
    house.insert("whole_sign".to_string(), 8u8);
    house.insert("placidus".to_string(), 7u8);

    let body = Body {
        id: BodyId::Sun,
        position: Position::from_longitude(251.206),
        ecliptic_latitude: jzod::Degrees8(-0.002),
        daily_speed: jzod::Degrees8(1.015),
        retrograde: false,
        distance_au: Some(0.986),
        house,
        antiscion: None,
        contra_antiscion: None,
    };
    let asc = Angle {
        id: AngleId::Ascendant,
        position: Position::from_longitude(58.261_667_55),
        antiscion: None,
        contra_antiscion: None,
    };

    let mut houses = jzod::Houses::new();
    let mut whole = jzod::HouseSystemCusps::new();
    whole.insert(1, jzod::HouseCusp::whole_sign_from_longitude(30.0));
    houses.insert("whole_sign".to_string(), whole);

    Chart {
        uid: "a3f8c2d1-6b94-4e17-8f53-2c71d0b43e85".into(),
        chart_type: ChartType::Radix,
        name: None,
        gender: Some("f".into()),
        rodden_rating: Some("AA".into()),
        birth: jzod::Birth {
            datetime: Datetime {
                year: 1895,
                month: 12,
                day: 3,
                hour: 15,
                minute: 15,
                second: 0,
                utc_offset: "+01:00".into(),
                iana_tz: Some("Europe/Vienna".into()),
                unknown: false,
                tod_method: None,
            },
            location: Location {
                name: Some("Vienna, Austria".into()),
                latitude: Some(48.208_333),
                longitude: Some(16.371_667),
            },
        },
        zodiac: Zodiac::Tropical,
        coordinate_system: CoordinateSystem::Geocentric,
        sect: Some(Sect::Diurnal),
        interp_sect_twilight: None,
        generator: Generator {
            name: "starcat".into(),
            version: "0.12.0".into(),
            components: vec![
                Component {
                    name: "pericynthion".into(),
                    version: "0.13.0".into(),
                },
                Component {
                    name: "jzod".into(),
                    version: "0.6.0".into(),
                },
            ],
        },
        ephemeris: Some(Ephemeris {
            sources: std::collections::BTreeMap::from([(
                "planets".to_string(),
                DataSource {
                    urls: vec![
                        "https://ssd.jpl.nasa.gov/ftp/eph/planets/Linux/de441/linux_m13000p17000.441"
                            .into(),
                    ],
                    cached: Some("linux_m13000p17000.441".into()),
                },
            )]),
            calculated_at: "2026-06-08T20:45:18Z".into(),
            jd_ut: Some(2_413_472.0),
            jd_tt: Some(2_413_472.0),
        }),
        placements: Placements {
            bodies: vec![body],
            angles: vec![asc],
            points: vec![],
            lots: vec![],
        },
        houses,
        lunar_phase: None,
        tithi: None,
        nested: vec![],
    }
}

#[test]
fn emitted_jzod_validates_against_schema() {
    let schema_src = include_str!("../schema/jzod-0.0.0.schema.json");
    let schema: serde_json::Value = serde_json::from_str(schema_src).expect("schema is valid JSON");
    let validator = jsonschema::validator_for(&schema).expect("schema compiles");

    let doc = JzodDocument::new(vec![full_chart()]);
    let json = jzod::to_string_pretty(&doc);
    let instance: serde_json::Value = serde_json::from_str(&json).unwrap();

    if let Err(errors) = validator.validate(&instance) {
        for e in errors {
            eprintln!("schema error at {}: {e}", e.instance_path);
        }
        panic!("emitted JZOD failed schema validation");
    }
}

#[test]
fn published_example_validates() {
    let schema_src = include_str!("../schema/jzod-0.0.0.schema.json");
    let schema: serde_json::Value = serde_json::from_str(schema_src).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();

    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/anna_freud_radix.json");
    let example_src = std::fs::read_to_string(path).expect("example fixture exists");
    let instance: serde_json::Value = serde_json::from_str(&example_src).unwrap();

    if let Err(errors) = validator.validate(&instance) {
        for e in errors {
            eprintln!("example schema error at {}: {e}", e.instance_path);
        }
        panic!("published example failed schema validation");
    }
}

#[test]
fn emitted_jzod_round_trips() {
    let doc = JzodDocument::new(vec![full_chart()]);
    let json = jzod::to_string_pretty(&doc);
    let back = jzod::from_str(&json).expect("valid JZOD");
    assert_eq!(doc, back);
}
