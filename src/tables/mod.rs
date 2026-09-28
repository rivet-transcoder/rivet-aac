//! The normative tables both halves of the codec share: the Huffman
//! codebooks (Annex A), the sampling-rate dependent tables (Tables 33, 35,
//! 38 and 45 to 57) and the window shapes (subclause 15.3.2), all from
//! ISO/IEC 13818-7:2004. Where each came from is in `docs/PROVENANCE.md`.

pub mod codebooks;
mod swb;
pub mod windows;

pub use swb::{RateTables, SAMPLING_FREQUENCIES, for_index, for_rate, index_for_explicit_rate};
