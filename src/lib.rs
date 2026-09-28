//! AAC-LC, written from ISO/IEC 13818-7 and ISO/IEC 14496-3.
//!
//! - [`encode`]: an AAC-LC encoder — raw access units and the
//!   AudioSpecificConfig, or ADTS.
//! - [`tables`]: the normative tables.
//!
//! PCM is interleaved `f32` at full scale ±1.0, in the native channel order.

pub mod encode;
mod error;
mod mdct;
pub mod tables;

pub use error::{Error, Result};

/// Samples per channel in one AAC-LC access unit.
pub const FRAME_SAMPLES: usize = 1024;
