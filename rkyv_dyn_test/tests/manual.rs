//! A trait object archived with hand-written impls instead of `archive_dyn`.

#[test]
fn manual_archive_dyn() {
    use ptr_meta::{DynMetadata, Pointee};
    use rkyv::{
        access_unchecked, deserialize,
        rancor::{Error, Fallible},
        to_bytes,
        traits::{ArchivePointee, LayoutRaw},
        Archive, ArchiveUnsized, Archived, ArchivedMetadata, Deserialize,
        DeserializeUnsized, Portable, Serialize, SerializeUnsized,
    };
    use rkyv_dyn::{
        register_trait_impls, ArchivedDynMetadata, AsDynDeserializer,
        AsDynSerializer, DeserializeDyn, DynDeserializer, DynSerializer,
        ImplId, RegisteredImpl, SerializeDyn,
    };

    pub trait Id {
        fn get_id(&self) -> i32;
    }

    #[ptr_meta::pointee]
    pub trait SerializeId<SE, DE>: Id + SerializeDyn<SE> {
        fn archived_impl_id(&self) -> ImplId;
    }

    impl<T, SE, DE> SerializeId<SE, DE> for T
    where
        T: Id + for<'a> Serialize<dyn DynSerializer<SE> + 'a>,
        T::Archived: RegisteredImpl<dyn DeserializeId<SE, DE>>,
    {
        fn archived_impl_id(&self) -> ImplId {
            T::Archived::IMPL_ID
        }
    }

    impl<SE, DE> ArchiveUnsized for dyn SerializeId<SE, DE> {
        type Archived = dyn DeserializeId<SE, DE>;

        fn archived_metadata(&self) -> ArchivedMetadata<Self> {
            ArchivedDynMetadata::new(self.archived_impl_id())
        }
    }

    impl<SE, DE> LayoutRaw for dyn SerializeId<SE, DE> {
        fn layout_raw(
            metadata: <Self as Pointee>::Metadata,
        ) -> Result<core::alloc::Layout, core::alloc::LayoutError> {
            Ok(metadata.layout())
        }
    }

    impl<S, DE> SerializeUnsized<S> for dyn SerializeId<S::Error, DE>
    where
        S: Fallible + AsDynSerializer<S::Error> + ?Sized,
    {
        fn serialize_unsized(
            &self,
            serializer: &mut S,
        ) -> Result<usize, S::Error> {
            self.serialize_dyn(serializer.as_dyn_serializer())
        }
    }

    #[ptr_meta::pointee]
    pub trait DeserializeId<SE, DE>:
        Id + DeserializeDyn<dyn SerializeId<SE, DE>, DE> + Portable
    {
    }

    impl<SE, DE> ArchivePointee for dyn DeserializeId<SE, DE> {
        type ArchivedMetadata = ArchivedDynMetadata<Self>;

        fn pointer_metadata(
            archived: &Self::ArchivedMetadata,
        ) -> <Self as Pointee>::Metadata {
            archived.lookup_metadata()
        }
    }

    impl<T, SE, DE> DeserializeId<SE, DE> for T where
        T: Id + DeserializeDyn<dyn SerializeId<SE, DE>, DE> + Portable
    {
    }

    impl<SE, D> DeserializeUnsized<dyn SerializeId<SE, D::Error>, D>
        for dyn DeserializeId<SE, D::Error>
    where
        D: Fallible + AsDynDeserializer<D::Error> + ?Sized,
    {
        unsafe fn deserialize_unsized(
            &self,
            deserializer: &mut D,
            out: *mut dyn SerializeId<SE, D::Error>,
        ) -> Result<(), <D as Fallible>::Error> {
            self.deserialize_dyn(deserializer.as_dyn_deserializer(), out)
        }

        fn deserialize_metadata(
            &self,
        ) -> <dyn SerializeId<SE, D::Error> as ptr_meta::Pointee>::Metadata
        {
            self.deserialized_pointer_metadata()
        }
    }

    #[derive(Archive, Serialize, Deserialize)]
    pub struct Test {
        id: i32,
    }

    impl Id for Test {
        fn get_id(&self) -> i32 {
            self.id
        }
    }

    register_trait_impls! {
        Archived<Test> as dyn DeserializeId<Error, Error>,
    }

    // Hack to make rustfmt happy
    type SId<SE, DE> = dyn SerializeId<SE, DE>;
    impl<SE, DE> DeserializeDyn<SId<SE, DE>, DE> for ArchivedTest
    where
        Archived<Test>: for<'a> Deserialize<Test, dyn DynDeserializer<DE> + 'a>,
        Archived<Test>: RegisteredImpl<dyn DeserializeId<SE, DE>>,
    {
        fn deserialize_dyn(
            &self,
            deserializer: &mut dyn DynDeserializer<DE>,
            out: *mut dyn SerializeId<SE, DE>,
        ) -> Result<(), DE> {
            unsafe {
                <Self as DeserializeUnsized<Test, _>>::deserialize_unsized(
                    self,
                    deserializer,
                    out.cast(),
                )
            }
        }

        fn deserialized_pointer_metadata(
            &self,
        ) -> DynMetadata<dyn SerializeId<SE, DE>> {
            ptr_meta::metadata(
                core::ptr::null::<Test>() as *const dyn SerializeId<SE, DE>
            )
        }
    }

    impl Id for ArchivedTest {
        fn get_id(&self) -> i32 {
            self.id.into()
        }
    }

    let value: Box<dyn SerializeId<Error, Error>> = Box::new(Test { id: 42 });

    let buf = to_bytes::<_>(&value).unwrap();
    type ArchivedType = Archived<Box<dyn SerializeId<Error, Error>>>;
    let archived_value =
        unsafe { access_unchecked::<ArchivedType>(buf.as_ref()) };
    assert_eq!(value.get_id(), archived_value.get_id());

    // look up the metadata again
    assert_eq!(value.get_id(), archived_value.get_id());
    assert_eq!(value.get_id(), archived_value.get_id());

    let deserialized_value: Box<dyn SerializeId<Error, Error>> =
        deserialize::<Box<dyn SerializeId<Error, Error>>, Error>(
            archived_value,
        )
        .unwrap();
    assert_eq!(value.get_id(), deserialized_value.get_id());
}
