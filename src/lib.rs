//! WorldShard game library.
//!
//! `worldgen` is engine-independent: it builds the island (shape, heights,
//! region colours, lava, lakes) from the same seed and recipe as the
//! design map, so every build of the game produces the same world.
pub mod worldgen;
