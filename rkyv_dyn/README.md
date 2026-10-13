# rkyv_dyn &emsp; [![Latest Version]][crates.io] [![License]][license path] [![requires: rustc 1.81+]][Rust 1.81]

[Latest Version]: https://img.shields.io/crates/v/rkyv_dyn.svg
[crates.io]: https://crates.io/crates/rkyv_dyn
[License]: https://img.shields.io/badge/license-MIT-blue.svg
[license path]: https://github.com/rkyv/rkyv/blob/master/LICENSE
[requires: rustc 1.81+]: https://img.shields.io/badge/rustc-1.81+-lightgray.svg
[Rust 1.81]: https://blog.rust-lang.org/2024/09/05/Rust-1.81.0.html 

Trait object serialization for rkyv.

# Resources

## Learning Materials

- The [rkyv book](https://rkyv.github.io/rkyv) covers the motivation, architecture, and major
  features of rkyv
- The [rkyv discord](https://discord.gg/65F6MdnbQh) is a great place to get help with specific issues and meet
  other people using rkyv

## Documentation

- [rkyv](https://docs.rs/rkyv), the core library
- [rkyv_dyn](https://docs.rs/rkyv_dyn), which adds trait object support to rkyv

## Benchmarks

- The [rust serialization benchmark](https://github.com/djkoloski/rust_serialization_benchmark) is a
  shootout style benchmark comparing many rust serialization solutions. It includes special
  benchmarks for zero-copy serialization solutions like rkyv.

## Sister Crates

- [bytecheck](https://github.com/rkyv/bytecheck), which rkyv uses for validation
- [ptr_meta](https://github.com/rkyv/ptr_meta), which rkyv uses for pointer manipulation
- [rend](https://github.com/rkyv/rend), which rkyv uses for endian-agnostic features

# Example

```rust
use rkyv::{rancor::Error, Archive, Archived, Deserialize, Serialize};
use rkyv_dyn::{archive_dyn, register_trait_impls};

#[archive_dyn(deserialize)]
trait Shape {
    fn area(&self) -> f32;
}

#[derive(Archive, Serialize, Deserialize)]
struct Square(f32);

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

type BoxedShape = Box<dyn SerializeShape<Error, Error>>;

fn main() {
    // Register every archived type with the archived trait it implements,
    // once, before serializing or accessing trait objects.
    register_trait_impls! {
        Archived<Square> as dyn DeserializeShape<Error, Error> = 0,
    }

    let value: BoxedShape = Box::new(Square(2.0));
    let bytes = rkyv::to_bytes::<Error>(&value).unwrap();

    let archived = rkyv::access::<Archived<BoxedShape>, Error>(&bytes).unwrap();
    assert_eq!(archived.area(), 4.0);

    let deserialized = rkyv::deserialize::<BoxedShape, Error>(archived).unwrap();
    assert_eq!(deserialized.area(), 4.0);
}
```
