//! `sfcht::parse_header` smoke tests.
//!
//! Hand-builds a well-formed 86-byte Solar Fire `.SFcht` file header per
//! `sfcht.ksy` and asserts the parser recovers `version`, `description`,
//! and `record_count` exactly. Also exercises the truncated-input error
//! path.
//!
//! Layout under test (little-endian throughout, ASCII strings space-padded):
//!
//! ```text
//! +0    u16       version
//! +2    char[80]  description
//! +82   u16       record_count
//! +84   u16       unknown (always 0 in observed files)
//! ```

use astrogram::error::ParseError;
use astrogram::sfcht::parse_header;

#[test]
fn parses_minimal_header() {
    let mut buf = vec![0u8; 86];
    buf[0..2].copy_from_slice(&3u16.to_le_bytes());

    let desc = b"TEST SPECIMEN";
    buf[2..2 + desc.len()].copy_from_slice(desc);
    for byte in &mut buf[2 + desc.len()..82] {
        *byte = b' ';
    }

    buf[82..84].copy_from_slice(&7u16.to_le_bytes());
    buf[84..86].copy_from_slice(&0u16.to_le_bytes());

    let header = parse_header(&buf).expect("well-formed 86-byte input must parse");

    assert_eq!(header.version, 3);
    assert_eq!(header.record_count, 7);
    assert_eq!(header.description, "TEST SPECIMEN");
}

#[test]
fn errors_on_short_input() {
    let buf = vec![0u8; 50];
    match parse_header(&buf) {
        Err(ParseError::Truncated {
            needed: 86,
            got: 50,
        }) => {}
        other => panic!("expected ParseError::Truncated{{86,50}}, got {other:?}"),
    }
}
