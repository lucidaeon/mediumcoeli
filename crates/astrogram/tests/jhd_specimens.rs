//! Structural checks over real Jagannatha Hora `.jhd` specimens. Skips cleanly
//! when `$ASTRO_SPECIMENS` is unset. Never asserts personal field values.

use std::path::{Path, PathBuf};

fn jhd_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            jhd_files(&p, out);
        } else if p.extension().and_then(|x| x.to_str()) == Some("jhd") {
            out.push(p);
        }
    }
}

#[test]
fn jhd_specimens_parse_and_hold_conventions() {
    let Ok(root) = std::env::var("ASTRO_SPECIMENS") else {
        eprintln!("ASTRO_SPECIMENS unset — skipping JHD specimen test");
        return;
    };
    let mut files = Vec::new();
    jhd_files(Path::new(&root), &mut files);
    if files.is_empty() {
        eprintln!("no .jhd specimens found — skipping");
        return;
    }
    for f in &files {
        let text = std::fs::read_to_string(f).expect("read jhd");
        let c = astrogram::jhd::parse_file(&text)
            .unwrap_or_else(|e| panic!("parse {:?}: {e}", f.file_name().unwrap()));
        // Structure only — no personal values.
        assert!((1..=12).contains(&c.month), "month in range");
        assert!((1..=31).contains(&c.day), "day in range");
        assert!(c.latitude.degrees().abs() <= 90.0, "lat sane");
        assert!(c.longitude.degrees().abs() <= 180.0, "lon sane");
        assert!(c.tz_offset_hours.abs() <= 14.0, "tz sane");
    }
    eprintln!("parsed {} JHD specimens", files.len());
}
