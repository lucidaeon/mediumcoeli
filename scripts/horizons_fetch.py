#!/usr/bin/env python3
"""
horizons_fetch.py — fetch reference positions from NASA JPL HORIZONS
for pericynthion's acceptance test charts and cache them as JSON.

Modes
-----
geocentric (default)
    Apparent geocentric ecliptic-of-date (quantity 31, CENTER='500@399').

topocentric
    Same quantity, observer located on Earth's surface at the chart's
    birth-place coordinates (CENTER='coord@399', SITE_COORD=lon,lat,elev).

heliocentric
    Apparent heliocentric ecliptic-of-date (CENTER='500@10'). Earth
    replaces the Sun in the body list; the Sun itself is skipped (zero
    distance from origin). Uses HORIZONS body 399 (Earth geocenter)
    to approximate pericynthion's Body::Earth (EMB − Moon/EMRAT), which
    differs by < 7" at most apparitions — acceptable for physics validation.

Usage
-----
    ./scripts/horizons_fetch.py                              # all charts, geocentric
    ./scripts/horizons_fetch.py --mode topocentric           # all charts, topocentric
    ./scripts/horizons_fetch.py --mode heliocentric          # all charts, heliocentric
    ./scripts/horizons_fetch.py lightning_strike             # one chart by id
    ./scripts/horizons_fetch.py --force                      # re-fetch even if cached
    ./scripts/horizons_fetch.py lightning_strike --mode topocentric --force

Fixtures
--------
    crates/pericynthion/tests/fixtures/horizons_{chart_id}_geo.json          geocentric
    crates/pericynthion/tests/fixtures/horizons_{chart_id}_topo.json       topocentric
    crates/pericynthion/tests/fixtures/horizons_{chart_id}_helio.json      heliocentric
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import urllib.parse
import urllib.request
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable, Optional

HORIZONS_API = "https://ssd.jpl.nasa.gov/api/horizons.api"

# HORIZONS body designators.
GEOCENTRIC_BODIES = {
    "Sun": "10",
    "Moon": "301",
    "Mercury": "199",
    "Venus": "299",
    "Mars": "499",
    "Jupiter": "599",
    "Saturn": "699",
    "Uranus": "799",
    "Neptune": "899",
    "Pluto": "999",
}

# Heliocentric: Earth replaces Sun. Sun itself is the origin (zero distance),
# HORIZONS would error on it, so skip. Use 399 (Earth geocenter) to approximate
# pericynthion's derived Body::Earth.
HELIOCENTRIC_BODIES = {
    "Earth": "399",
    "Moon": "301",
    "Mercury": "199",
    "Venus": "299",
    "Mars": "499",
    "Jupiter": "599",
    "Saturn": "699",
    "Uranus": "799",
    "Neptune": "899",
    "Pluto": "999",
}


@dataclass
class ObserverLocation:
    """Observer location for topocentric and heliocentric fixtures."""
    lat_deg: float      # North positive
    lon_e_deg: float    # East positive
    elev_km: float


@dataclass
class Chart:
    """A reference chart: id, descriptive name, UT instant, and notes."""
    id: str
    name: str
    iso_ut: str  # ISO 8601 UT instant (no TZ; HORIZONS treats as UT)
    notes: str
    observer: ObserverLocation = field(default=None)


# Each entry here must have a corresponding approved docs/ref_*.md file.
# Adding a new chart requires adding the ref doc first.
CHARTS: list[Chart] = [
    Chart(
        id="vettius_valens",
        name="Vettius Valens",
        # 0120-02-08 18:35 LMT Antioch (36°07'E) → UT 16:10:32
        # ref: docs/ref_vettius_valens_porphyry.md
        iso_ut="0120-02-08 16:10:38",
        notes="Julian calendar, LMT Antioch (36°09'25\" E)",
        observer=ObserverLocation(
            lat_deg=36.2,       # 36°12'N
            lon_e_deg=36.157,   # 36°09'25"E — same as LMT longitude
            elev_km=0.085,
        ),
    ),
    Chart(
        id="william_lilly",
        name="William Lilly",
        # 1602-05-11 02:00 LMT Diseworth (001°11'W) → UT 02:04:44
        # ref: docs/ref_william_lilly_regiomontanus.md
        iso_ut="1602-05-11 02:05:19",
        notes="1602-05-11 Gregorian (= 1602-05-01 Julian), LMT Diseworth, England",
        observer=ObserverLocation(
            lat_deg=52.8166118,   # 52°48'60"N
            lon_e_deg=-1.3281652, # 001°19'41"W
            elev_km=0.060,
        ),
    ),
    Chart(
        id="lightning_strike",
        name="Lightning Strike",
        # 1955-11-12 22:04 PST (UT-8) → UT 1955-11-13 06:04:00
        # ref: docs/ref_lightning_strike_placidus.md
        iso_ut="1955-11-13 06:04:00",
        notes="Gregorian, Universal City CA, PST",
        observer=ObserverLocation(
            lat_deg=34.1389,    # 34°08'20"N
            lon_e_deg=-118.3525, # 118°21'09"W
            elev_km=0.165,
        ),
    ),
    Chart(
        id="anna_freud",
        name="Anna Freud",
        # 1895-12-03 15:15 CET (UTC+1) → UT 14:15:00
        # ref: docs/ref_anna_freud_alcabitius.md
        iso_ut="1895-12-03 14:15:00",
        notes="Gregorian, Vienna CET",
        observer=ObserverLocation(
            lat_deg=48.2167,    # 48°13'N
            lon_e_deg=16.3333,  # 16°20'E
            elev_km=0.171,
        ),
    ),
    Chart(
        id="adele_haenel",
        name="Adèle Haenel",
        # 1989-02-11 16:20 CET (UTC+1) → UT 15:20:00
        # ref: docs/ref_adele_haenel_whole.md
        iso_ut="1989-02-11 15:20:00",
        notes="Gregorian, Paris CET",
        observer=ObserverLocation(
            lat_deg=48.8667,    # 48°52'N
            lon_e_deg=2.1167,   # 002°07'E
            elev_km=0.035,
        ),
    ),
    Chart(
        id="unix_overflow_2038",
        name="UNIX 32-bit Overflow",
        # 2038-01-19 03:14:07 UTC
        # ref: docs/ref_unix_overflow_helio.md
        iso_ut="2038-01-19 03:14:07",
        notes="UNIX 32-bit overflow moment, London UTC",
        observer=ObserverLocation(
            lat_deg=51.5,       # 51°30'N
            lon_e_deg=-0.1667,  # 000°10'W
            elev_km=0.011,
        ),
    ),
]

FIXTURE_DIR = (
    Path(__file__).resolve().parent.parent
    / "crates"
    / "pericynthion"
    / "tests"
    / "fixtures"
)


MONTH_ABBR = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun",
    "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
]


def iso_to_horizons(iso: str) -> str:
    """Convert YYYY-MM-DD HH:MM:SS to HORIZONS YYYY-MMM-DD HH:MM:SS format."""
    m = re.fullmatch(r"(-?\d{1,4})-(\d{2})-(\d{2})[ T](\d{2}):(\d{2})(?::(\d{2}))?", iso)
    if not m:
        raise ValueError(f"unrecognized ISO: {iso!r}")
    y, mo, d, h, mi = (int(m.group(i)) for i in range(1, 6))
    s = int(m.group(6)) if m.group(6) else 0
    return f"{y:04d}-{MONTH_ABBR[mo - 1]}-{d:02d} {h:02d}:{mi:02d}:{s:02d}"


def bump_iso_minute(iso: str) -> str:
    """Return `iso` with one minute added (no TZ, no DST awareness)."""
    m = re.fullmatch(r"(-?\d{1,4})-(\d{2})-(\d{2})[ T](\d{2}):(\d{2})(?::(\d{2}))?", iso)
    if not m:
        raise ValueError(f"unrecognized ISO: {iso!r}")
    y, mo, d, h, mi = (int(m.group(i)) for i in range(1, 6))
    s = int(m.group(6)) if m.group(6) else 0
    mi += 1
    if mi == 60:
        mi = 0
        h += 1
    return f"{y:04d}-{mo:02d}-{d:02d} {h:02d}:{mi:02d}:{s:02d}"


def parse_horizons_obs(text: str) -> tuple[float, float]:
    """Extract ecliptic lon/lat from a HORIZONS OBSERVER text response."""
    soe = text.find("$$SOE")
    eoe = text.find("$$EOE")
    if soe < 0 or eoe < 0:
        raise RuntimeError(
            "HORIZONS response missing $$SOE/$$EOE markers — likely an API "
            f"error. First 500 chars:\n{text[:500]}"
        )
    data_block = text[soe + len("$$SOE"):eoe].strip()
    first_row = data_block.splitlines()[0].strip()
    cols = first_row.split()
    eclat = float(cols[-1])
    eclon = float(cols[-2])
    return eclon, eclat


def _build_params(body_id: str, iso_ut: str, center: str,
                  extra: Optional[dict] = None) -> dict:
    start = iso_to_horizons(iso_ut)
    stop_dt = iso_to_horizons(bump_iso_minute(iso_ut))
    params = {
        "format": "text",
        "COMMAND": f"'{body_id}'",
        "OBJ_DATA": "'NO'",
        "MAKE_EPHEM": "'YES'",
        "EPHEM_TYPE": "'OBSERVER'",
        "CENTER": f"'{center}'",
        "START_TIME": f"'{start}'",
        "STOP_TIME": f"'{stop_dt}'",
        "STEP_SIZE": "'1m'",
        "QUANTITIES": "'31'",
        "REF_SYSTEM": "'ICRF'",
        "CAL_FORMAT": "'BOTH'",
        "ANG_FORMAT": "'DEG'",
    }
    if extra:
        params.update(extra)
    return params


def fetch_one_geocentric(body_id: str, iso_ut: str) -> dict:
    params = _build_params(body_id, iso_ut, center="500@399")
    url = HORIZONS_API + "?" + urllib.parse.urlencode(params)
    with urllib.request.urlopen(url, timeout=30) as resp:
        text = resp.read().decode("utf-8")
    eclon, eclat = parse_horizons_obs(text)
    return {"longitude_deg": eclon, "latitude_deg": eclat}


def fetch_one_topocentric(body_id: str, iso_ut: str, obs: ObserverLocation) -> dict:
    # HORIZONS SITE_COORD: East-longitude degrees, North-latitude degrees, elevation km.
    site = f"'{obs.lon_e_deg},{obs.lat_deg},{obs.elev_km}'"
    params = _build_params(
        body_id, iso_ut,
        center="coord@399",
        extra={"SITE_COORD": site},
    )
    url = HORIZONS_API + "?" + urllib.parse.urlencode(params)
    with urllib.request.urlopen(url, timeout=30) as resp:
        text = resp.read().decode("utf-8")
    eclon, eclat = parse_horizons_obs(text)
    return {"longitude_deg": eclon, "latitude_deg": eclat}


def parse_horizons_vectors(text: str) -> tuple[float, float, float]:
    """Extract X, Y, Z (AU) from a HORIZONS VECTORS text response."""
    soe = text.find("$$SOE")
    eoe = text.find("$$EOE")
    if soe < 0 or eoe < 0:
        raise RuntimeError(
            "HORIZONS VECTORS response missing $$SOE/$$EOE markers — likely an "
            f"API error. First 500 chars:\n{text[:500]}"
        )
    block = text[soe + len("$$SOE"):eoe].strip()
    for line in block.splitlines():
        m = re.match(
            r"\s*X\s*=\s*([+-]?\d+\.\d+[Ee][+-]\d+)\s+"
            r"Y\s*=\s*([+-]?\d+\.\d+[Ee][+-]\d+)\s+"
            r"Z\s*=\s*([+-]?\d+\.\d+[Ee][+-]\d+)",
            line,
        )
        if m:
            return float(m.group(1)), float(m.group(2)), float(m.group(3))
    raise RuntimeError(
        f"could not find X Y Z line in HORIZONS VECTORS response.\n"
        f"First 1000 chars:\n{text[:1000]}"
    )


def fetch_one_heliocentric(body_id: str, iso_ut: str) -> dict:
    """Fetch geometric heliocentric J2000/ICRF position vector (AU) from HORIZONS.

    Uses EPHEM_TYPE='VECTORS', CENTER='500@10' (Sun), VEC_CORR='NONE' (geometric,
    no light-time correction). Returns x_au, y_au, z_au in the ICRF frame at J2000.

    The caller (acceptance test) applies our own precession+nutation to rotate from
    J2000 to ecliptic-of-date, then compares against heliocentric_ecliptic_position.
    This validates the barycentric position subtraction (body_bary − sun_bary) while
    keeping the rotation step identical in both pipelines.
    """
    start = iso_to_horizons(iso_ut)
    stop_dt = iso_to_horizons(bump_iso_minute(iso_ut))
    params = {
        "format": "text",
        "COMMAND": f"'{body_id}'",
        "OBJ_DATA": "'NO'",
        "MAKE_EPHEM": "'YES'",
        "EPHEM_TYPE": "'VECTORS'",
        "CENTER": "'500@10'",
        "START_TIME": f"'{start}'",
        "STOP_TIME": f"'{stop_dt}'",
        "STEP_SIZE": "'1m'",
        "OUT_UNITS": "'AU-D'",
        "REF_SYSTEM": "'ICRF'",
        "REF_PLANE": "'FRAME'",   # equatorial ICRF, not ecliptic (ecliptic is the default)
        "VEC_CORR": "'NONE'",
        "CAL_FORMAT": "'BOTH'",
    }
    url = HORIZONS_API + "?" + urllib.parse.urlencode(params)
    with urllib.request.urlopen(url, timeout=30) as resp:
        text = resp.read().decode("utf-8")
    x, y, z = parse_horizons_vectors(text)
    return {"x_au": x, "y_au": y, "z_au": z}


def fetch_chart(chart: Chart, mode: str) -> dict:
    if mode == "topocentric" and chart.observer is None:
        raise ValueError(f"chart {chart.id!r} has no observer location for topocentric fetch")

    if mode == "geocentric":
        bodies_map = GEOCENTRIC_BODIES
        fetch_fn = lambda name, hid: fetch_one_geocentric(hid, chart.iso_ut)
        source_note = "NASA JPL HORIZONS, quantity 31 (ObsEcLon/ObsEcLat, of-date), geocentric"
    elif mode == "topocentric":
        bodies_map = GEOCENTRIC_BODIES
        fetch_fn = lambda name, hid: fetch_one_topocentric(hid, chart.iso_ut, chart.observer)
        source_note = (
            "NASA JPL HORIZONS, quantity 31 (ObsEcLon/ObsEcLat, of-date), topocentric "
            f"lat={chart.observer.lat_deg} lon_e={chart.observer.lon_e_deg} "
            f"elev_km={chart.observer.elev_km}"
        )
    elif mode == "heliocentric":
        bodies_map = HELIOCENTRIC_BODIES
        fetch_fn = lambda name, hid: fetch_one_heliocentric(hid, chart.iso_ut)
        source_note = (
            "NASA JPL HORIZONS VECTORS, geometric heliocentric J2000/ICRF "
            "(CENTER='500@10', VEC_CORR='NONE', OUT_UNITS='AU-D'). "
            "x_au/y_au/z_au are barycentric-body minus barycentric-Sun at the "
            "requested UT instant. Apply precession+nutation to obtain ecliptic-of-date."
        )
    else:
        raise ValueError(f"unknown mode: {mode!r}")

    print(f"=== {chart.name} ({chart.iso_ut} UT) [{mode}] ===")
    body_results = {}
    skipped = {}
    for name, hid in bodies_map.items():
        try:
            print(f"  ⇩ {name}  ", end="", flush=True)
            result = fetch_fn(name, hid)
            body_results[name] = result
            if "longitude_deg" in result:
                print(f"lon={result['longitude_deg']:.4f}° lat={result['latitude_deg']:+.4f}°")
            else:
                print(f"x={result['x_au']:+.6f} y={result['y_au']:+.6f} z={result['z_au']:+.6f} AU")
        except RuntimeError as e:
            skipped[name] = str(e).split("\n")[-1][:200]
            print(f"    SKIPPED: {skipped[name]}")

    out: dict = {
        "chart_id": chart.id,
        "name": chart.name,
        "iso_ut": chart.iso_ut,
        "notes": chart.notes,
        "source": source_note,
        "bodies": body_results,
        "skipped": skipped,
    }
    if mode == "topocentric" and chart.observer:
        out["observer"] = {
            "lat_deg": chart.observer.lat_deg,
            "lon_e_deg": chart.observer.lon_e_deg,
            "elev_km": chart.observer.elev_km,
        }
    return out


def fixture_path(chart: Chart, mode: str) -> Path:
    suffix = {"geocentric": "_geo", "topocentric": "_topo", "heliocentric": "_helio"}[mode]
    return FIXTURE_DIR / f"horizons_{chart.id}{suffix}.json"


def main(argv: Iterable[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("chart_id", nargs="?", help="only fetch this chart by id")
    parser.add_argument(
        "--mode", default="geocentric",
        choices=["geocentric", "topocentric", "heliocentric"],
        help="coordinate mode (default: geocentric)",
    )
    parser.add_argument("--force", action="store_true", help="re-fetch even if cached")
    args = parser.parse_args(list(argv))

    FIXTURE_DIR.mkdir(parents=True, exist_ok=True)

    selected = [c for c in CHARTS if args.chart_id in (None, c.id)]
    if not selected:
        print(f"unknown chart id: {args.chart_id!r}", file=sys.stderr)
        return 2

    for chart in selected:
        path = fixture_path(chart, args.mode)
        if path.exists() and not args.force:
            print(f"=== {chart.name} [{args.mode}]: already cached at {path} (use --force to refresh)")
            continue
        try:
            data = fetch_chart(chart, args.mode)
        except Exception as e:
            print(f"FAILED for {chart.id}: {e}", file=sys.stderr)
            return 1
        path.write_text(json.dumps(data, indent=2) + "\n")
        print(f"  → wrote {path}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
