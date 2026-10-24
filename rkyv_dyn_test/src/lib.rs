//! Test suite for rkyv_dyn. The tests are in `tests/`, one binary per set of
//! registered impls.
//!
//! This library is the upstream crate of `tests/external.rs`, which registers
//! impls of a trait and types defined here.

use rkyv::{rancor::Error, Archive, Archived, Deserialize, Serialize};
use rkyv_dyn::{archive_dyn, register_trait_impls};

#[archive_dyn(deserialize)]
pub trait Shape {
    fn area(&self) -> f32;
}

/// Registered in this crate.
#[derive(Archive, Serialize, Deserialize)]
pub struct Square(pub f32);

#[archive_dyn(deserialize)]
impl Shape for Square {
    fn area(&self) -> f32 {
        self.0 * self.0
    }
}

impl Shape for ArchivedSquare {
    fn area(&self) -> f32 {
        self.0.to_native() * self.0.to_native()
    }
}

/// Not registered in this crate.
#[derive(Archive, Serialize, Deserialize)]
pub struct Rect(pub f32, pub f32);

#[archive_dyn(deserialize)]
impl Shape for Rect {
    fn area(&self) -> f32 {
        self.0 * self.1
    }
}

impl Shape for ArchivedRect {
    fn area(&self) -> f32 {
        self.0.to_native() * self.1.to_native()
    }
}

register_trait_impls! {
    Archived<Square> as dyn DeserializeShape<Error, Error> = 0,
}
