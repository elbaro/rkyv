//! Calling `register_trait_impls!` more than once.

use rkyv::{Archive, Archived, Serialize};
use rkyv_dyn::{archive_dyn, register_trait_impls};

#[archive_dyn]
pub trait Marker {}

#[derive(Archive, Serialize)]
pub struct Unit;

impl Marker for Unit {}
impl Marker for ArchivedUnit {}

fn register() {
    register_trait_impls! {
        Archived<Unit> as dyn ArchivedMarker,
    }
}

#[test]
#[should_panic(expected = "register_trait_impls! was called more than once")]
fn register_twice() {
    register();
    register();
}
