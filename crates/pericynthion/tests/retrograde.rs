use pericynthion::coords::signed_daily_motion;

// --- signed_daily_motion ---

#[test]
fn forward_motion_simple() {
    // 10° advance = +10°
    assert!((signed_daily_motion(100.0, 110.0) - 10.0).abs() < 1e-9);
}

#[test]
fn retrograde_motion_simple() {
    // 5° retreat = -5°
    assert!((signed_daily_motion(100.0, 95.0) - (-5.0)).abs() < 1e-9);
}

#[test]
fn wraps_forward_across_360_seam() {
    // 359° → 1°: raw = -358°, interpreted as +2°
    assert!((signed_daily_motion(359.0, 1.0) - 2.0).abs() < 1e-9);
}

#[test]
fn wraps_retrograde_across_360_seam() {
    // 1° → 359°: raw = +358°, interpreted as -2°
    assert!((signed_daily_motion(1.0, 359.0) - (-2.0)).abs() < 1e-9);
}
