//! NASA JPL planetary-ephemeris file handling.
//!
//! This module is the I/O and on-disk-layout boundary of the library.
//! It knows nothing about astronomy beyond what JPL itself documents:
//!
//! - [`header`] parses the ASCII `header.NNN` (layout + named constants).
//! - [`reader`] memory-maps the binary `linux_*.NNN` / `xnp_*.NNN` and
//!   serves one coefficient record at a time, in native byte order.
//! - [`ascii`] parses `ascp*.NNN` / `ascm*.NNN` text chunks through the same
//!   [`reader::RecordSource`] trait as the binary reader.
//! - [`discover`] locates the best DE dataset (binary or ASCII) from any node
//!   in the JPL mirror hierarchy up to 8 levels deep.
//! - [`oracle`] holds the hardcoded BLAKE3 + size manifest of the full
//!   `ssd.jpl.nasa.gov/ftp/eph/` mirror for bit-exact integrity verification.
//!
//! Per-body slot slicing and Chebyshev evaluation happen one layer up
//! in [`crate::ephemeris`]; coordinate transformations, ΔT, and
//! calendar handling live elsewhere again.

pub mod ascii;
pub mod discover;
pub mod header;
pub mod oracle;
pub mod reader;
