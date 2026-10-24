//! Impl IDs given to `register_trait_impls!`.

use rkyv::{
    access_unchecked, deserialize, rancor::Error, to_bytes, Archive, Archived,
    Deserialize, Serialize,
};
use rkyv_dyn::{archive_dyn, register_trait_impls, ImplId, TraitImpl};

#[archive_dyn(deserialize)]
pub trait Id {
    fn get_id(&self) -> i32;
}

macro_rules! id_type {
    ($name:ident, $archived:ident, $id:literal) => {
        #[derive(Archive, Serialize, Deserialize)]
        pub struct $name;

        #[archive_dyn(deserialize)]
        impl Id for $name {
            fn get_id(&self) -> i32 {
                $id
            }
        }

        impl Id for $archived {
            fn get_id(&self) -> i32 {
                $id
            }
        }
    };
}

id_type!(A, ArchivedA, 1);
id_type!(B, ArchivedB, 2);
id_type!(C, ArchivedC, 3);

type BoxedId = Box<dyn SerializeId<Error, Error>>;
type Trait = dyn DeserializeId<Error, Error>;

register_trait_impls! {
    Archived<A> as dyn DeserializeId<Error, Error> = 7,
    Archived<B> as dyn DeserializeId<Error, Error> = 8,
    Archived<C> as dyn DeserializeId<Error, Error> = 2,
}

fn impl_id<T: 'static>() -> ImplId {
    TraitImpl::find_by_type::<T, Trait>().unwrap().impl_id()
}

#[test]
fn explicit_impl_ids() {
    assert_eq!(impl_id::<Archived<A>>(), 7);
    assert_eq!(impl_id::<Archived<B>>(), 8);
    assert_eq!(impl_id::<Archived<C>>(), 2);
    for impl_id in [2, 7, 8] {
        assert_eq!(
            TraitImpl::find::<Trait>(impl_id).unwrap().impl_id(),
            impl_id
        );
    }
    assert!(TraitImpl::find::<Trait>(0).is_none());

    for (value, id) in [
        (Box::new(A) as BoxedId, 1),
        (Box::new(B), 2),
        (Box::new(C), 3),
    ] {
        let bytes = to_bytes::<Error>(&value).unwrap();
        let archived = unsafe { access_unchecked::<Archived<BoxedId>>(&bytes) };
        assert_eq!(archived.get_id(), id);
        let deserialized = deserialize::<BoxedId, Error>(archived).unwrap();
        assert_eq!(deserialized.get_id(), id);
    }
}
