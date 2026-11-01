//! # Location Inference, Gazetteer Matching & Directory Heuristics
//!
//! Provides the submodules responsible for geographic location resolution:
//! - [`gazetteer`]: Fast in-memory spatial index and trigram fuzzy matching for global avian biodiversity hotspots.
//! - [`path_parser`]: Extracts candidate location tokens from folder hierarchies, stripping dates, camera models, and noise words.
//! - [`resolver`]: Coordinates the multi-level cascade from GPS coordinates to directory gazetteers.

pub mod gazetteer;
pub mod path_parser;
pub mod resolver;

pub use gazetteer::*;
pub use path_parser::*;
pub use resolver::*;
