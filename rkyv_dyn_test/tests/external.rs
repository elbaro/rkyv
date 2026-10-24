//! Impls registered outside the crate that defines the trait.

#![cfg(feature = "bytecheck")]

use rkyv::{
    access, deserialize, rancor::Error, to_bytes, Archive, Archived,
    Deserialize, Serialize,
};
use rkyv_dyn::{archive_dyn, register_trait_impls};
use rkyv_dyn_test::{DeserializeShape, Rect, SerializeShape, Shape, Square};

/// A type implementing the upstream trait.
#[derive(Archive, Serialize, Deserialize)]
pub struct Circle(f32);

#[archive_dyn(deserialize)]
impl Shape for Circle {
    fn area(&self) -> f32 {
        3.0 * self.0 * self.0
    }
}

impl Shape for ArchivedCircle {
    fn area(&self) -> f32 {
        3.0 * self.0.to_native() * self.0.to_native()
    }
}

/// A type implementing the upstream trait that is never registered.
#[derive(Archive, Serialize, Deserialize)]
pub struct Unregistered;

#[archive_dyn(deserialize)]
impl Shape for Unregistered {
    fn area(&self) -> f32 {
        0.0
    }
}

impl Shape for ArchivedUnregistered {
    fn area(&self) -> f32 {
        0.0
    }
}

// Both the trait and `Rect` are defined upstream.
register_trait_impls! {
    Archived<Circle> as dyn DeserializeShape<Error, Error> = 1,
    Archived<Rect> as dyn DeserializeShape<Error, Error> = 2,
}

type BoxedShape = Box<dyn SerializeShape<Error, Error>>;

#[test]
fn round_trip() {
    let value: Vec<BoxedShape> = vec![
        Box::new(Square(2.0)),
        Box::new(Circle(1.0)),
        Box::new(Rect(2.0, 3.0)),
    ];
    let bytes = to_bytes::<Error>(&value).unwrap();
    let archived = access::<Archived<Vec<BoxedShape>>, Error>(&bytes).unwrap();
    let areas = archived
        .iter()
        .map(|shape| shape.area())
        .collect::<Vec<_>>();
    assert_eq!(areas, [4.0, 3.0, 6.0]);

    let deserialized = deserialize::<Vec<BoxedShape>, Error>(archived).unwrap();
    let areas = deserialized
        .iter()
        .map(|shape| shape.area())
        .collect::<Vec<_>>();
    assert_eq!(areas, [4.0, 3.0, 6.0]);
}

#[test]
fn unregistered() {
    let value: BoxedShape = Box::new(Unregistered);
    let error = to_bytes::<Error>(&value).unwrap_err();
    if cfg!(debug_assertions) {
        assert!(error.to_string().contains("is not registered"), "{error}");
    }
}
