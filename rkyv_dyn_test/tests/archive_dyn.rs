//! Trait objects archived with `archive_dyn`.

use std::{
    rc::Rc,
    sync::{Arc, Once},
};

use rkyv::{
    access_unchecked, access_unchecked_mut, deserialize, rancor::Error,
    to_bytes, Archive, Archived, Deserialize, Serialize,
};
use rkyv_dyn::{archive_dyn, register_trait_impls};

// ─── Traits ──────────────────────────────────────────────────────────────────

#[archive_dyn(deserialize)]
pub trait Id {
    fn get_id(&self) -> i32;
}

#[archive_dyn(serialize = "STestTrait", deserialize = "DTestTrait")]
pub trait TestTrait<T> {
    fn get_value(&self) -> T;
}

/// A trait without `deserialize`, so its archived trait is `ArchivedCounter`.
#[archive_dyn]
pub trait Counter {
    fn count(&self) -> i32;
    fn set_count(&mut self, count: i32);
}

// ─── Types ───────────────────────────────────────────────────────────────────

#[derive(Archive, Serialize, Deserialize)]
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

#[derive(Archive, Serialize, Deserialize)]
pub struct NamedId {
    name: String,
    id: i32,
}

#[archive_dyn(deserialize)]
impl Id for NamedId {
    fn get_id(&self) -> i32 {
        self.id + self.name.len() as i32
    }
}

impl Id for ArchivedNamedId {
    fn get_id(&self) -> i32 {
        self.id.to_native() + self.name.len() as i32
    }
}

#[derive(Archive, Serialize, Deserialize)]
pub struct Test<T> {
    value: T,
}

#[archive_dyn(serialize = "STestTrait", deserialize = "DTestTrait")]
impl TestTrait<i32> for Test<i32> {
    fn get_value(&self) -> i32 {
        self.value
    }
}

impl TestTrait<i32> for ArchivedTest<i32> {
    fn get_value(&self) -> i32 {
        self.value.to_native()
    }
}

#[archive_dyn(serialize = "STestTrait", deserialize = "DTestTrait")]
impl TestTrait<String> for Test<String> {
    fn get_value(&self) -> String {
        self.value.clone()
    }
}

impl TestTrait<String> for ArchivedTest<String> {
    fn get_value(&self) -> String {
        self.value.to_string()
    }
}

#[derive(Archive, Serialize)]
pub struct Count(i32);

#[archive_dyn]
impl Counter for Count {
    fn count(&self) -> i32 {
        self.0
    }

    fn set_count(&mut self, count: i32) {
        self.0 = count;
    }
}

impl Counter for ArchivedCount {
    fn count(&self) -> i32 {
        self.0.to_native()
    }

    fn set_count(&mut self, count: i32) {
        self.0 = count.into();
    }
}

/// A struct with trait object fields.
#[derive(Archive, Serialize, Deserialize)]
pub struct Container {
    boxed: Box<dyn SerializeId<Error, Error>>,
    list: Vec<Box<dyn SerializeId<Error, Error>>>,
    maybe: Option<Box<dyn SerializeId<Error, Error>>>,
    shared: Vec<Arc<dyn SerializeId<Error, Error>>>,
}

// ─── Registration ────────────────────────────────────────────────────────────

fn register() {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        register_trait_impls! {
            Archived<IntId> as dyn DeserializeId<Error, Error>,
            Archived<NamedId> as dyn DeserializeId<Error, Error>,
            Archived<Test<i32>> as dyn DTestTrait<i32, Error, Error>,
            Archived<Test<String>> as dyn DTestTrait<String, Error, Error>,
            Archived<Count> as dyn ArchivedCounter,
        }
    });
}

// ─── Tests ───────────────────────────────────────────────────────────────────

type BoxedId = Box<dyn SerializeId<Error, Error>>;

#[test]
fn boxed() {
    register();

    for value in [
        Box::new(IntId(42)) as BoxedId,
        Box::new(NamedId {
            name: "hello".to_string(),
            id: 10,
        }),
    ] {
        let bytes = to_bytes::<Error>(&value).unwrap();
        let archived = unsafe { access_unchecked::<Archived<BoxedId>>(&bytes) };
        assert_eq!(archived.get_id(), value.get_id());

        let deserialized = deserialize::<BoxedId, Error>(archived).unwrap();
        assert_eq!(deserialized.get_id(), value.get_id());
    }
}

#[test]
fn custom_names_and_generic_trait() {
    register();

    let int_value: Box<dyn STestTrait<i32, Error, Error>> =
        Box::new(Test { value: 42 });
    let string_value: Box<dyn STestTrait<String, Error, Error>> =
        Box::new(Test {
            value: "hello world".to_string(),
        });

    let int_bytes = to_bytes::<Error>(&int_value).unwrap();
    let int_archived = unsafe {
        access_unchecked::<Archived<Box<dyn STestTrait<i32, Error, Error>>>>(
            &int_bytes,
        )
    };
    assert_eq!(int_archived.get_value(), 42);
    let int_deserialized: Box<dyn STestTrait<i32, Error, Error>> =
        deserialize::<_, Error>(int_archived).unwrap();
    assert_eq!(int_deserialized.get_value(), 42);

    let string_bytes = to_bytes::<Error>(&string_value).unwrap();
    let string_archived = unsafe {
        access_unchecked::<Archived<Box<dyn STestTrait<String, Error, Error>>>>(
            &string_bytes,
        )
    };
    assert_eq!(string_archived.get_value(), "hello world");
    let string_deserialized: Box<dyn STestTrait<String, Error, Error>> =
        deserialize::<_, Error>(string_archived).unwrap();
    assert_eq!(string_deserialized.get_value(), "hello world");
}

#[test]
fn without_deserialize() {
    register();

    let value: Box<dyn SerializeCounter<Error>> = Box::new(Count(10));
    let bytes = to_bytes::<Error>(&value).unwrap();
    let archived = unsafe {
        access_unchecked::<Archived<Box<dyn SerializeCounter<Error>>>>(&bytes)
    };
    assert_eq!(archived.count(), 10);
}

#[test]
fn mutable() {
    register();

    let value: Box<dyn SerializeCounter<Error>> = Box::new(Count(10));
    let mut bytes = to_bytes::<Error>(&value).unwrap();
    let archived = unsafe {
        access_unchecked_mut::<Archived<Box<dyn SerializeCounter<Error>>>>(
            &mut bytes,
        )
    };
    let counter = rkyv::boxed::ArchivedBox::get_seal(archived);
    // SAFETY: `set_count` keeps the archived value valid.
    unsafe { counter.unseal_unchecked() }.set_count(64);

    let archived = unsafe {
        access_unchecked::<Archived<Box<dyn SerializeCounter<Error>>>>(&bytes)
    };
    assert_eq!(archived.count(), 64);
}

#[test]
fn arc() {
    register();

    let value: Arc<dyn SerializeId<Error, Error>> = Arc::new(IntId(42));
    let bytes = to_bytes::<Error>(&value).unwrap();
    let archived = unsafe {
        access_unchecked::<Archived<Arc<dyn SerializeId<Error, Error>>>>(&bytes)
    };
    assert_eq!(archived.get_id(), 42);

    let deserialized: Arc<dyn SerializeId<Error, Error>> =
        deserialize::<_, Error>(archived).unwrap();
    assert_eq!(deserialized.get_id(), 42);
}

#[test]
fn rc() {
    register();

    let value: Rc<dyn SerializeId<Error, Error>> = Rc::new(NamedId {
        name: "rc".to_string(),
        id: 1,
    });
    let bytes = to_bytes::<Error>(&value).unwrap();
    let archived = unsafe {
        access_unchecked::<Archived<Rc<dyn SerializeId<Error, Error>>>>(&bytes)
    };
    assert_eq!(archived.get_id(), 3);

    let deserialized: Rc<dyn SerializeId<Error, Error>> =
        deserialize::<_, Error>(archived).unwrap();
    assert_eq!(deserialized.get_id(), 3);
}

#[test]
fn weak_access() {
    register();

    let strong: Arc<dyn SerializeId<Error, Error>> = Arc::new(IntId(7));
    let value = (strong.clone(), Arc::downgrade(&strong));
    let bytes = to_bytes::<Error>(&value).unwrap();
    type Value = (
        Arc<dyn SerializeId<Error, Error>>,
        std::sync::Weak<dyn SerializeId<Error, Error>>,
    );
    let archived = unsafe { access_unchecked::<Archived<Value>>(&bytes) };
    assert_eq!(archived.0.get_id(), 7);
    assert_eq!(archived.1.upgrade().unwrap().get_id(), 7);
    // rkyv can't deserialize `Weak<T>` for unsized `T` because there is no
    // `Weak::new` for unsized types.
}

#[test]
fn container() {
    register();

    let shared: Arc<dyn SerializeId<Error, Error>> = Arc::new(IntId(5));
    let value = Container {
        boxed: Box::new(IntId(1)),
        list: vec![
            Box::new(IntId(2)),
            Box::new(NamedId {
                name: "three".to_string(),
                id: -2,
            }),
        ],
        maybe: Some(Box::new(IntId(4))),
        shared: vec![shared.clone(), shared],
    };
    let bytes = to_bytes::<Error>(&value).unwrap();
    let archived = unsafe { access_unchecked::<ArchivedContainer>(&bytes) };
    assert_eq!(archived.boxed.get_id(), 1);
    let list = archived
        .list
        .iter()
        .map(|id| id.get_id())
        .collect::<Vec<_>>();
    assert_eq!(list, [2, 3]);
    assert_eq!(archived.maybe.as_ref().unwrap().get_id(), 4);
    // Both `Arc`s point to the same archived value.
    assert!(std::ptr::addr_eq(
        archived.shared[0].get(),
        archived.shared[1].get(),
    ));

    let deserialized = deserialize::<Container, Error>(archived).unwrap();
    assert_eq!(deserialized.boxed.get_id(), 1);
    let list = deserialized
        .list
        .iter()
        .map(|id| id.get_id())
        .collect::<Vec<_>>();
    assert_eq!(list, [2, 3]);
    assert_eq!(deserialized.maybe.unwrap().get_id(), 4);
    assert_eq!(deserialized.shared[0].get_id(), 5);
    // Deserializing shared pointers preserves sharing.
    assert!(Arc::ptr_eq(
        &deserialized.shared[0],
        &deserialized.shared[1]
    ));
}

#[test]
fn none() {
    register();

    let value: Option<BoxedId> = None;
    let bytes = to_bytes::<Error>(&value).unwrap();
    let archived =
        unsafe { access_unchecked::<Archived<Option<BoxedId>>>(&bytes) };
    assert!(archived.is_none());
    let deserialized = deserialize::<Option<BoxedId>, Error>(archived).unwrap();
    assert!(deserialized.is_none());
}
