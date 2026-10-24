//! Registering two impls of a trait with the same impl ID.

use rkyv::{rancor::Error, to_bytes, Archive, Archived, Serialize};
use rkyv_dyn::{archive_dyn, register_trait_impls};

#[archive_dyn]
pub trait Marker {}

#[derive(Archive, Serialize)]
pub struct A;

impl Marker for A {}
impl Marker for ArchivedA {}

#[derive(Archive, Serialize)]
pub struct B;

impl Marker for B {}
impl Marker for ArchivedB {}

register_trait_impls! {
    Archived<A> as dyn ArchivedMarker = 0,
    Archived<B> as dyn ArchivedMarker = 0,
}

#[test]
#[should_panic(expected = "impl ID 0 is registered for both")]
fn duplicate_impl_ids() {
    let value: Box<dyn SerializeMarker<Error>> = Box::new(A);
    let _ = to_bytes::<Error>(&value);
}
