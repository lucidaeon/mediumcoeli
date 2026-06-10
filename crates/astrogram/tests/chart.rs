use astrogram::chart::{
    Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, Zodiac,
};
use astrogram::error::ChartError;

// --- Longitude ---

#[test]
fn longitude_accepts_boundaries() {
    assert!(Longitude::new(-180.0).is_ok());
    assert!(Longitude::new(0.0).is_ok());
    assert!(Longitude::new(180.0).is_ok());
}

#[test]
fn longitude_rejects_out_of_range() {
    assert!(Longitude::new(180.001).is_err());
    assert!(Longitude::new(-180.001).is_err());
    assert!(Longitude::new(360.0).is_err());
}

#[test]
fn longitude_preserves_degrees() {
    let lon = Longitude::new(-73.9857).unwrap();
    assert!((lon.degrees() - -73.9857).abs() < 1e-10);
}

#[test]
fn longitude_out_of_range_error_carries_value() {
    match Longitude::new(999.0) {
        Err(ChartError::LongitudeOutOfRange(v)) => assert!((v - 999.0).abs() < 1e-10),
        other => panic!("expected LongitudeOutOfRange, got {other:?}"),
    }
}

// --- Latitude ---

#[test]
fn latitude_accepts_boundaries() {
    assert!(Latitude::new(-90.0).is_ok());
    assert!(Latitude::new(0.0).is_ok());
    assert!(Latitude::new(90.0).is_ok());
}

#[test]
fn latitude_rejects_out_of_range() {
    assert!(Latitude::new(90.001).is_err());
    assert!(Latitude::new(-90.001).is_err());
}

#[test]
fn latitude_preserves_degrees() {
    let lat = Latitude::new(40.7128).unwrap();
    assert!((lat.degrees() - 40.7128).abs() < 1e-10);
}

// --- Chart construction ---

#[test]
fn chart_constructs_with_required_fields() {
    let chart = Chart {
        name: "Test Chart".to_string(),
        secondary_name: None,
        city: Some("New York".to_string()),
        region: Some("NY, USA".to_string()),
        longitude: Longitude::new(-74.006).unwrap(),
        latitude: Latitude::new(40.7128).unwrap(),
        year: 1990,
        month: 6,
        day: 15,
        hour: 12,
        minute: 0,
        second: 0,
        tz_offset_hours: -5.0,
        tz_abbreviation: Some("EST".to_string()),
        is_lmt: false,
        event_type: EventType::Male,
        source_rating: None,
        house_system: HouseSystem::Placidus,
        zodiac: Zodiac::Tropical,
        coordinate_system: CoordinateSystem::Geocentric,
        sub_charts: vec![],
        notes: None,
    };

    assert_eq!(chart.name, "Test Chart");
    assert_eq!(chart.year, 1990);
    assert!((chart.longitude.degrees() - -74.006).abs() < 1e-10);
}

#[test]
fn chart_supports_bce_years() {
    let chart = Chart {
        name: "Julius Caesar".to_string(),
        secondary_name: None,
        city: None,
        region: None,
        longitude: Longitude::new(12.4964).unwrap(),
        latitude: Latitude::new(41.9028).unwrap(),
        year: -100,
        month: 7,
        day: 13,
        hour: 0,
        minute: 0,
        second: 0,
        tz_offset_hours: 0.0,
        tz_abbreviation: None,
        is_lmt: true,
        event_type: EventType::Male,
        source_rating: None,
        house_system: HouseSystem::WholeSign,
        zodiac: Zodiac::Tropical,
        coordinate_system: CoordinateSystem::Geocentric,
        sub_charts: vec![],
        notes: None,
    };

    assert_eq!(chart.year, -100);
    assert!(chart.is_lmt);
}

#[test]
fn chart_house_system_other_preserves_id() {
    match HouseSystem::Other(42) {
        HouseSystem::Other(id) => assert_eq!(id, 42),
        _ => panic!("expected Other variant"),
    }
}
