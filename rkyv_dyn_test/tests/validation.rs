//! Validation of archived trait objects.

#![cfg(feature = "bytecheck")]

use std::sync::{Arc, Once};

use rkyv::{
    access, munge::munge, rancor::Error, to_bytes, Archive, Archived, Place,
    Portable, Serialize,
};
use rkyv_dyn::{archive_dyn, register_trait_impls, RegisteredImpl};

// ─── Traits ──────────────────────────────────────────────────────────────────

#[archive_dyn(deserialize)]
pub trait Id {
    fn get_id(&self) -> i32;
}

#[archive_dyn]
pub trait Other {}

// ─── Types ───────────────────────────────────────────────────────────────────

#[derive(Archive, Serialize, rkyv::Deserialize)]
pub struct IntId(i32);

#[archive_dyn(deserialize)]
impl Id for IntId {
    fn get_id(&self) -> i32 {
        self.0
    }
}

impl Id for ArchivedIntId {
    fn get_id(&self) -> i32 {
        self.0.to_native()
    }
}

/// A type whose archived form has invalid bit patterns.
#[derive(Archive, Serialize, rkyv::Deserialize)]
pub struct Flag(bool);

#[archive_dyn(deserialize)]
impl Id for Flag {
    fn get_id(&self) -> i32 {
        self.0 as i32
    }
}

impl Id for ArchivedFlag {
    fn get_id(&self) -> i32 {
        self.0 as i32
    }
}

/// A type whose archived form does not implement `CheckBytes`.
pub struct Unchecked(i32);

#[derive(Portable)]
#[repr(transparent)]
pub struct ArchivedUnchecked(Archived<i32>);

impl Archive for Unchecked {
    type Archived = ArchivedUnchecked;
    type Resolver = ();

    fn resolve(&self, _: Self::Resolver, out: Place<Self::Archived>) {
        munge!(let ArchivedUnchecked(id) = out);
        self.0.resolve((), id);
    }
}

impl<S: rkyv::rancor::Fallible + ?Sized> Serialize<S> for Unchecked {
    fn serialize(&self, _: &mut S) -> Result<Self::Resolver, S::Error> {
        Ok(())
    }
}

impl<D: rkyv::rancor::Fallible + ?Sized> rkyv::Deserialize<Unchecked, D>
    for ArchivedUnchecked
{
    fn deserialize(&self, _: &mut D) -> Result<Unchecked, D::Error> {
        Ok(Unchecked(self.0.to_native()))
    }
}

#[archive_dyn(deserialize)]
impl Id for Unchecked {
    fn get_id(&self) -> i32 {
        self.0
    }
}

impl Id for ArchivedUnchecked {
    fn get_id(&self) -> i32 {
        self.0.to_native()
    }
}

#[derive(Archive, Serialize)]
pub struct Unit;

impl Other for Unit {}
impl Other for ArchivedUnit {}

// ─── Registration ────────────────────────────────────────────────────────────

fn register() {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        register_trait_impls! {
            Archived<IntId> as dyn DeserializeId<Error, Error>,
            Archived<Flag> as dyn DeserializeId<Error, Error>,
            Archived<Unchecked> as dyn DeserializeId<Error, Error>,
            Archived<Unit> as dyn ArchivedOther,
        }
    });
}

// ─── Tests ───────────────────────────────────────────────────────────────────

type BoxedId = Box<dyn SerializeId<Error, Error>>;

/// Returns the bytes of the impl ID of the archived root `Box<dyn ...>`.
fn root_impl_id(bytes: &mut [u8]) -> &mut [u8] {
    let len = bytes.len();
    // An archived box is a relative pointer: an offset followed by the
    // archived impl ID.
    &mut bytes[len - size_of::<Archived<rkyv_dyn::ImplId>>()..]
}

fn set_root_impl_id(bytes: &mut [u8], impl_id: rkyv_dyn::ImplId) {
    let impl_id = Archived::<rkyv_dyn::ImplId>::from_native(impl_id);
    // SAFETY: `Archived<ImplId>` has no padding.
    let impl_id_bytes = unsafe {
        core::slice::from_raw_parts(
            (&impl_id as *const Archived<rkyv_dyn::ImplId>).cast::<u8>(),
            size_of_val(&impl_id),
        )
    };
    root_impl_id(bytes).copy_from_slice(impl_id_bytes);
}

/// Asserts that validating `bytes` as a `BoxedId` fails with an error
/// containing `message`.
///
/// `rancor::Error` only keeps error messages with debug assertions enabled, so
/// the message is only checked then.
fn assert_access_error(bytes: &[u8], message: &str) {
    match access::<Archived<BoxedId>, Error>(bytes) {
        Ok(_) => panic!("validation passed"),
        Err(error) => {
            if cfg!(debug_assertions) {
                let error = error.to_string();
                assert!(error.contains(message), "unexpected error: {error}");
            }
        }
    }
}

#[test]
fn valid() {
    register();

    let value: BoxedId = Box::new(IntId(42));
    let bytes = to_bytes::<Error>(&value).unwrap();
    let archived = access::<Archived<BoxedId>, Error>(&bytes).unwrap();
    assert_eq!(archived.get_id(), 42);
}

#[test]
fn valid_shared() {
    register();

    let shared: Arc<dyn SerializeId<Error, Error>> = Arc::new(IntId(3));
    let value = vec![shared.clone(), shared];
    let bytes = to_bytes::<Error>(&value).unwrap();
    type Value = Vec<Arc<dyn SerializeId<Error, Error>>>;
    let archived = access::<Archived<Value>, Error>(&bytes).unwrap();
    assert_eq!(archived[0].get_id(), 3);
    assert_eq!(archived[1].get_id(), 3);
}

#[test]
fn invalid_value() {
    register();

    let value: BoxedId = Box::new(Flag(true));
    let mut bytes = to_bytes::<Error>(&value).unwrap();
    assert!(access::<Archived<BoxedId>, Error>(&bytes).is_ok());

    // The boxed `ArchivedFlag` is serialized first, at the start of the buffer.
    assert_eq!(bytes[0], 1);
    bytes[0] = 2;
    assert_access_error(&bytes, "bool set to invalid byte 2");
}

#[test]
fn unchecked_type() {
    register();

    let value: BoxedId = Box::new(Unchecked(42));
    let bytes = to_bytes::<Error>(&value).unwrap();
    assert_access_error(&bytes, "does not implement CheckBytes");
}

#[test]
fn unregistered_impl_id() {
    register();

    let value: BoxedId = Box::new(IntId(42));
    let mut bytes = to_bytes::<Error>(&value).unwrap();
    set_root_impl_id(&mut bytes, 1000);
    assert_access_error(&bytes, "impl ID 1000 is not registered");
}

#[test]
fn impl_id_of_other_trait() {
    register();

    let value: BoxedId = Box::new(IntId(42));
    let mut bytes = to_bytes::<Error>(&value).unwrap();
    let other_id =
        <Archived<Unit> as RegisteredImpl<dyn ArchivedOther>>::IMPL_ID;
    set_root_impl_id(&mut bytes, other_id);
    assert_access_error(&bytes, "is registered for a different trait");
}

#[test]
fn impl_id_of_other_type() {
    register();

    // Swapping the impl ID checks the bytes of the `ArchivedIntId` as an
    // `ArchivedFlag`. Every byte of the ID is 42, which is not a valid `bool`.
    let value: BoxedId = Box::new(IntId(0x2a2a2a2a));
    let mut bytes = to_bytes::<Error>(&value).unwrap();
    let flag_id = <Archived<Flag> as RegisteredImpl<
        dyn DeserializeId<Error, Error>,
    >>::IMPL_ID;
    set_root_impl_id(&mut bytes, flag_id);
    assert_access_error(&bytes, "bool set to invalid byte 42");
}
