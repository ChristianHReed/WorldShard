//! WorldShard game library.
//!
//! `worldgen` is engine-independent: it builds the island (shape, heights,
//! region colours, lava, lakes) from the same seed and recipe as the
//! design map, so every build of the game produces the same world.
pub mod lod;
pub mod bridge;
pub mod city;
pub mod landmarks;
pub mod meshgen;
pub mod places;
pub mod save;
pub mod sky;
pub mod worldgen;
