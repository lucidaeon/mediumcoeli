use std::{
    env,
    fs::{self, File},
    io::{Read, Write},
    path::Path,
};

use flate2::read::GzDecoder;

// Full text of the CDS ReadMe for catalog V/50 (Hoffleit & Warren 1991),
// embedded verbatim so provenance travels with the generated source.
const BSC5_README: &str = r#"V/50           Bright Star Catalogue, 5th Revised Ed.     (Hoffleit+, 1991)
================================================================================
The Bright Star Catalogue,  5th Revised Ed. (Preliminary Version)
     Hoffleit D., Warren Jr W.H.
    <Astronomical Data Center, NSSDC/ADC (1991)>
    =1964BS....C......0H
    =1991bsc..book.....H
================================================================================
ADC_Keywords: Combined data ; Stars, bright


Description (prepared by Wayne H. Warren Jr., 1991 June 28):

    The  Bright  Star  Catalogue  (BSC) is widely used as a source of
    basic astronomical and astrophysical data for stars brighter than
    magnitude 6.5.   The  catalog  contains  the  identifications  of
    included stars in several other widely-used catalogs, double- and
    multiple-star  identifications,  indication  of  variability  and
    variable-star identifiers, equatorial positions for  B1900.0  and
    J2000.0,  galactic  coordinates,  UBVRI photoelectric photometric
    data when they exist, spectral types on  the  Morgan-Keenan  (MK)
    classification   system,   proper  motions  (J2000.0),  parallax,
    radial-   and   rotational-velocity   data,   and   multiple-star
    information  (number  of  components,  separation,  and magnitude
    differences) for known nonsingle stars.  In addition to the  data
    file, there is an extensive remarks file that gives more detailed
    information  on  individual  entries.   This information includes
    star  names,  colors,  spectra,   variability   details,   binary
    characteristics,  radial  and rotational velocities for companion
    stars,  duplicity  information,  dynamical  parallaxes,   stellar
    dimensions (radii and diameters), polarization, and membership in
    stellar groups and clusters.  The existence of remarks is flagged
    in the main data file.

    The  BSC  contains  9110  objects,  of  which  9096 are stars (14
    objects catalogued in the original compilation of 1908 are  novae
    or  extragalactic objects that have been retained to preserve the
    numbering, but most of their data are omitted), while the remarks
    section is slightly larger than the main catalog.    The  present
    edition of the compilation includes many new data and the remarks
    section has been enlarged considerably.

    This  preliminary version of the fifth edition of the Bright Star
    Catalogue supersedes the published and machine-readable  versions
    of  Hoffleit  (1982, Yale University Observatory) and is intended
    for use until the final version of this edition is completed.  It
    has  been  made  available  only   for   dissemination   on   the
    Astronomical Data Center CD ROM.

    The  brief  format description applies to the preliminary version
    of the catalog only.   The  format  will  change  for  the  final
    edition.


Author's addresses:
    Dorrit Hoffleit
        Department of Astronomy
        Yale University
    Wayne H. Warren Jr.
        ST Systems Corporation
        National Space Science Data Center
        NASA Goddard Space Flight Center


File Summary:
--------------------------------------------------------------------------------
 FileName    Lrecl    Records    Explanations
--------------------------------------------------------------------------------
ReadMe          80          .    This file
catalog        197       9110    The main part of the Catalogue
notes          132       9190    Remarks
--------------------------------------------------------------------------------

See also:
    V/36 : Supplement to the Bright Star Catalogue  (Hoffleit+ 1983)

Byte-by-byte Description of file: catalog
--------------------------------------------------------------------------------
   Bytes Format  Units   Label    Explanations
--------------------------------------------------------------------------------
   1-  4  I4     ---     HR       [1/9110]+ Harvard Revised Number
                                    = Bright Star Number
   5- 14  A10    ---     Name     Name, generally Bayer and/or Flamsteed name
  15- 25  A11    ---     DM       Durchmusterung Identification (zone in
                                    bytes 17-19)
  26- 31  I6     ---     HD       [1/225300]? Henry Draper Catalog Number
  32- 37  I6     ---     SAO      [1/258997]? SAO Catalog Number
  38- 41  I4     ---     FK5      ? FK5 star Number
      42  A1     ---     IRflag   [I] I if infrared source
      43  A1     ---   r_IRflag  *[ ':] Coded reference for infrared source
      44  A1     ---    Multiple *[AWDIRS] Double or multiple-star code
  45- 49  A5     ---     ADS      Aitken's Double Star Catalog (ADS) designation
  50- 51  A2     ---     ADScomp  ADS number components
  52- 60  A9     ---     VarID    Variable star identification
  61- 62  I2     h       RAh1900  ?Hours RA, equinox B1900, epoch 1900.0
  63- 64  I2     min     RAm1900  ?Minutes RA, equinox B1900, epoch 1900.0
  65- 68  F4.1   s       RAs1900  ?Seconds RA, equinox B1900, epoch 1900.0
      69  A1     ---     DE-1900  ?Sign Dec, equinox B1900, epoch 1900.0
  70- 71  I2     deg     DEd1900  ?Degrees Dec, equinox B1900, epoch 1900.0
  72- 73  I2     arcmin  DEm1900  ?Minutes Dec, equinox B1900, epoch 1900.0
  74- 75  I2     arcsec  DEs1900  ?Seconds Dec, equinox B1900, epoch 1900.0
  76- 77  I2     h       RAh      ?Hours RA, equinox J2000, epoch 2000.0
  78- 79  I2     min     RAm      ?Minutes RA, equinox J2000, epoch 2000.0
  80- 83  F4.1   s       RAs      ?Seconds RA, equinox J2000, epoch 2000.0
      84  A1     ---     DE-      ?Sign Dec, equinox J2000, epoch 2000.0
  85- 86  I2     deg     DEd      ?Degrees Dec, equinox J2000, epoch 2000.0
  87- 88  I2     arcmin  DEm      ?Minutes Dec, equinox J2000, epoch 2000.0
  89- 90  I2     arcsec  DEs      ?Seconds Dec, equinox J2000, epoch 2000.0
  91- 96  F6.2   deg     GLON     ?Galactic longitude
  97-102  F6.2   deg     GLAT     ?Galactic latitude
 103-107  F5.2   mag     Vmag     ?Visual magnitude
     108  A1     ---   n_Vmag    *[ HR] Visual magnitude code
     109  A1     ---   u_Vmag     [ :?] Uncertainty flag on V
 110-114  F5.2   mag     B-V      ? B-V color in the UBV system
     115  A1     ---   u_B-V      [ :?] Uncertainty flag on B-V
 116-120  F5.2   mag     U-B      ? U-B color in the UBV system
     121  A1     ---   u_U-B      [ :?] Uncertainty flag on U-B
 122-126  F5.2   mag     R-I      ? R-I in system specified by n_R-I
     127  A1     ---   n_R-I      [CE:?D] Code for R-I system (Cousin, Eggen)
 128-147  A20    ---     SpType   Spectral type
     148  A1     ---   n_SpType   [evt] Spectral type code
 149-154  F6.3 arcsec/yr pmRA    *?Annual proper motion in RA J2000, FK5 system
 155-160  F6.3 arcsec/yr pmDE     ?Annual proper motion in Dec J2000, FK5 system
     161  A1     ---   n_Parallax [D] D indicates a dynamical parallax,
                                    otherwise a trigonometric parallax
 162-166  F5.3   arcsec  Parallax ? Trigonometric parallax
 167-170  I4     km/s    RadVel   ? Heliocentric Radial Velocity
 171-174  A4     ---   n_RadVel  *[V?SB123O ] Radial velocity comments
 175-176  A2     ---   l_RotVel   [<=> ] Rotational velocity limit characters
 177-179  I3     km/s    RotVel   ? Rotational velocity, v sin i
     180  A1     ---   u_RotVel   [ :v] uncertainty and variability flag
 181-184  F4.1   mag     Dmag     ? Magnitude difference of double
 185-190  F6.1   arcsec  Sep      ? Separation of components in Dmag
 191-194  A4     ---     MultID   Identifications of components in Dmag
 195-196  I2     ---     MultCnt  ? Number of components assigned to a multiple
     197  A1     ---     NoteFlag [*] a star indicates that there is a note
--------------------------------------------------------------------------------
Note on pmRA:
     As usually assumed, the proper motion in RA is the projected
     motion (cos(DE).d(RA)/dt).

Historical Notes:
  * 02-Oct-1993 at CDS (Francois Ochsenbein)
    Corrections inserted from the CD-ROM version
    "Selected Astronomical Catalogs, Volume 1, 1991".
  * 02-Nov-1995 at CDS (Francois Ochsenbein):
    Documentation slightly changed to accommodate to standards.
================================================================================
(End)                                Francois Ochsenbein     [CDS]   02-Nov-1995"#;

fn parse_f64(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() { None } else { t.parse().ok() }
}

fn parse_i32(s: &str) -> Option<i32> {
    let t = s.trim();
    if t.is_empty() { None } else { t.parse().ok() }
}

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace_root = Path::new(&manifest_dir).parent().unwrap().parent().unwrap();
    let catalog_gz = workspace_root.join("catalog.gz");

    println!("cargo:rerun-if-changed={}", catalog_gz.display());

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest = Path::new(&out_dir).join("bsc5_generated.rs");
    let mut out = File::create(&dest).unwrap();

    if !catalog_gz.exists() {
        // Compile clean without the catalog; BSC5_CATALOG is empty.
        // Run `just fetch bsc5` to download catalog.gz and rebuild.
        writeln!(
            out,
            "// catalog.gz not found — run `just fetch bsc5` to populate BSC5_CATALOG.\n\
             #[allow(missing_docs)]\n\
             pub static BSC5_CATALOG: &[BscEntry] = &[];"
        )
        .unwrap();
        return;
    }

    // Decompress
    let mut gz = GzDecoder::new(File::open(&catalog_gz).unwrap());
    let mut raw = Vec::new();
    gz.read_to_end(&mut raw).unwrap();

    // Write provenance header
    writeln!(out, "// ---- BSC5 PROVENANCE ----").unwrap();
    for line in BSC5_README.lines() {
        writeln!(out, "// {line}").unwrap();
    }
    writeln!(out, "// ---- END PROVENANCE ----\n").unwrap();

    writeln!(
        out,
        "#[allow(missing_docs, clippy::unreadable_literal, clippy::approx_constant)]\npub static BSC5_CATALOG: &[BscEntry] = &["
    )
    .unwrap();

    let mut count = 0u32;
    for record in raw.split(|&b| b == b'\n') {
        if record.len() < 90 {
            continue;
        }

        // All fields are ASCII; safe to slice as bytes.
        let s = |a: usize, b: usize| -> &str {
            std::str::from_utf8(&record[a..b.min(record.len())]).unwrap_or("")
        };

        let hr_str = s(0, 4).trim();
        if hr_str.is_empty() {
            continue;
        }
        let hr: u16 = match hr_str.parse() {
            Ok(v) => v,
            Err(_) => continue,
        };

        let name = s(4, 14).trim();

        // J2000 RA: bytes 76-83 (0-indexed 75-82)
        let ra_h = parse_i32(s(75, 77));
        let ra_m = parse_i32(s(77, 79));
        let ra_s = parse_f64(s(79, 83));

        // J2000 Dec: bytes 84-90 (0-indexed 83-89)
        let dec_sign = record.get(83).copied().unwrap_or(b' ');
        let dec_d = parse_i32(s(84, 86));
        let dec_m = parse_i32(s(86, 88));
        let dec_s = parse_i32(s(88, 90));

        // V magnitude: bytes 103-107 (0-indexed 102-106)
        let vmag = if record.len() > 107 {
            parse_f64(s(102, 107))
        } else {
            None
        };

        // Proper motion: bytes 149-160 (0-indexed 148-159)
        let pm_ra = if record.len() > 154 {
            parse_f64(s(148, 154))
        } else {
            None
        };
        let pm_dec = if record.len() > 160 {
            parse_f64(s(154, 160))
        } else {
            None
        };

        // Skip entries without coordinates (the 14 non-stellar objects)
        let (Some(ra_h), Some(ra_m), Some(ra_s), Some(dec_d), Some(dec_m), Some(dec_s)) =
            (ra_h, ra_m, ra_s, dec_d, dec_m, dec_s)
        else {
            continue;
        };

        let ra_deg = (ra_h as f64 + ra_m as f64 / 60.0 + ra_s / 3600.0) * 15.0;
        let dec_abs = dec_d as f64 + dec_m as f64 / 60.0 + dec_s as f64 / 3600.0;
        let dec_deg = if dec_sign == b'-' { -dec_abs } else { dec_abs };

        let vmag_str = match vmag {
            Some(v) => format!("Some({v:.2}_f32)"),
            None => "None".to_string(),
        };
        let pm_ra_str = match pm_ra {
            Some(v) => format!("Some({v:.3}_f32)"),
            None => "None".to_string(),
        };
        let pm_dec_str = match pm_dec {
            Some(v) => format!("Some({v:.3}_f32)"),
            None => "None".to_string(),
        };

        // Escape any backslash or quote in name (names are plain ASCII, but be safe)
        let name_escaped = name.replace('\\', "\\\\").replace('"', "\\\"");

        writeln!(
            out,
            "    BscEntry {{ hr: {hr}, name: \"{name_escaped}\", \
             ra_deg: {ra_deg:.6}, dec_deg: {dec_deg:.6}, \
             vmag: {vmag_str}, pm_ra: {pm_ra_str}, pm_dec: {pm_dec_str} }},"
        )
        .unwrap();

        count += 1;
    }

    writeln!(out, "]; // {count} entries").unwrap();

    // Stamp the source file size so Cargo re-runs if the gz is replaced
    let gz_size = fs::metadata(&catalog_gz).map(|m| m.len()).unwrap_or(0);
    writeln!(
        out,
        "// catalog.gz size at generation time: {gz_size} bytes"
    )
    .unwrap();
}
