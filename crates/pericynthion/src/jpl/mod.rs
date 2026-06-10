//! NASA JPL planetary-ephemeris file handling.
//!
//! This module is the I/O and on-disk-layout boundary of the library.
//! It knows nothing about astronomy beyond what JPL itself documents:
//!
//! - [`header`] parses the ASCII `header.NNN` (layout + named constants).
//! - [`reader`] memory-maps the binary `linux_*.NNN` / `xnp_*.NNN` and
//!   serves one coefficient record at a time, in native byte order.
//! - [`discover`] finds the highest-numbered header/binary pair in a
//!   data directory.
//!
//! Per-body slot slicing and Chebyshev evaluation happen one layer up
//! in [`crate::ephemeris`]; coordinate transformations, ΔT, and
//! calendar handling live elsewhere again.

pub mod discover;
pub mod header;
pub mod reader;
