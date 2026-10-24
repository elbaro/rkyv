//! Validation implementations and helper types.

use core::{
    alloc::Layout, any::TypeId, error::Error, fmt, marker::PhantomData,
    ops::Range,
};

use bytecheck::{CheckBytes, Verify};
use ptr_meta::{DynMetadata, Pointee};
use rancor::{fail, BoxedError, Fallible, Source, Strategy};
use rkyv::{
    de::{ErasedPtr, Metadata},
    validation::{shared::ValidationState, ArchiveContext, SharedContext},
};

use crate::{ArchivedDynMetadata, ImplId, TraitImpl, TraitImpls};

/// An object-safe validation context.
///
/// Archived trait objects are checked through this trait because the concrete
/// type behind a trait object, and therefore its `CheckBytes` impl, is only
/// known at runtime.
pub trait DynContext {
    /// See [`ArchiveContext::check_subtree_ptr`].
    fn check_subtree_ptr_dyn(
        &mut self,
        ptr: *const u8,
        layout: &Layout,
    ) -> Result<(), BoxedError>;

    /// See [`ArchiveContext::push_subtree_range`].
    ///
    /// # Safety
    ///
    /// `root` and `end` must be located inside the archive.
    unsafe fn push_subtree_range_dyn(
        &mut self,
        root: *const u8,
        end: *const u8,
    ) -> Result<Range<usize>, BoxedError>;

    /// See [`ArchiveContext::pop_subtree_range`].
    ///
    /// # Safety
    ///
    /// `range` must be a range returned from this validator.
    unsafe fn pop_subtree_range_dyn(
        &mut self,
        range: Range<usize>,
    ) -> Result<(), BoxedError>;

    /// See [`SharedContext::start_shared`].
    fn start_shared_dyn(
        &mut self,
        shared_type_id: TypeId,
        ptr: ErasedPtr,
        metadata_is_eq: unsafe fn(Metadata, Metadata) -> bool,
    ) -> Result<ValidationState, BoxedError>;

    /// See [`SharedContext::finish_shared`].
    fn finish_shared_dyn(
        &mut self,
        shared_type_id: TypeId,
        ptr: ErasedPtr,
    ) -> Result<(), BoxedError>;
}

/// Implements [`DynContext`] for a validation context.
struct DynContextAdapter<'a, C: ?Sized>(&'a mut C);

impl<C> DynContext for DynContextAdapter<'_, C>
where
    C: Fallible + ArchiveContext + SharedContext + ?Sized,
    C::Error: Source,
{
    fn check_subtree_ptr_dyn(
        &mut self,
        ptr: *const u8,
        layout: &Layout,
    ) -> Result<(), BoxedError> {
        self.0
            .check_subtree_ptr(ptr, layout)
            .map_err(<BoxedError as Source>::new)
    }

    unsafe fn push_subtree_range_dyn(
        &mut self,
        root: *const u8,
        end: *const u8,
    ) -> Result<Range<usize>, BoxedError> {
        // SAFETY: The caller has upheld the safety requirements of
        // `push_subtree_range`.
        unsafe { self.0.push_subtree_range(root, end) }
            .map_err(<BoxedError as Source>::new)
    }

    unsafe fn pop_subtree_range_dyn(
        &mut self,
        range: Range<usize>,
    ) -> Result<(), BoxedError> {
        // SAFETY: The caller has upheld the safety requirements of
        // `pop_subtree_range`.
        unsafe { self.0.pop_subtree_range(range) }
            .map_err(<BoxedError as Source>::new)
    }

    fn start_shared_dyn(
        &mut self,
        shared_type_id: TypeId,
        ptr: ErasedPtr,
        metadata_is_eq: unsafe fn(Metadata, Metadata) -> bool,
    ) -> Result<ValidationState, BoxedError> {
        self.0
            .start_shared(shared_type_id, ptr, metadata_is_eq)
            .map_err(<BoxedError as Source>::new)
    }

    fn finish_shared_dyn(
        &mut self,
        shared_type_id: TypeId,
        ptr: ErasedPtr,
    ) -> Result<(), BoxedError> {
        self.0
            .finish_shared(shared_type_id, ptr)
            .map_err(<BoxedError as Source>::new)
    }
}

/// A sized validation context that forwards to a [`DynContext`].
///
/// This is the context that the archived types behind trait objects are
/// checked with.
pub struct DynValidator<'a>(&'a mut dyn DynContext);

// SAFETY: `DynValidator` forwards every call to a `DynContext`, which forwards
// every call to an `ArchiveContext` that upholds the safety requirements.
unsafe impl ArchiveContext<BoxedError> for DynValidator<'_> {
    fn check_subtree_ptr(
        &mut self,
        ptr: *const u8,
        layout: &Layout,
    ) -> Result<(), BoxedError> {
        self.0.check_subtree_ptr_dyn(ptr, layout)
    }

    unsafe fn push_subtree_range(
        &mut self,
        root: *const u8,
        end: *const u8,
    ) -> Result<Range<usize>, BoxedError> {
        // SAFETY: The caller has upheld the safety requirements of
        // `push_subtree_range`.
        unsafe { self.0.push_subtree_range_dyn(root, end) }
    }

    unsafe fn pop_subtree_range(
        &mut self,
        range: Range<usize>,
    ) -> Result<(), BoxedError> {
        // SAFETY: The caller has upheld the safety requirements of
        // `pop_subtree_range`.
        unsafe { self.0.pop_subtree_range_dyn(range) }
    }
}

impl SharedContext<BoxedError> for DynValidator<'_> {
    fn start_shared(
        &mut self,
        shared_type_id: TypeId,
        ptr: ErasedPtr,
        metadata_is_eq: unsafe fn(Metadata, Metadata) -> bool,
    ) -> Result<ValidationState, BoxedError> {
        self.0.start_shared_dyn(shared_type_id, ptr, metadata_is_eq)
    }

    fn finish_shared(
        &mut self,
        shared_type_id: TypeId,
        ptr: ErasedPtr,
    ) -> Result<(), BoxedError> {
        self.0.finish_shared_dyn(shared_type_id, ptr)
    }
}

/// The validation context that the archived types behind trait objects are
/// checked with.
pub type DynCheckContext<'a> = Strategy<DynValidator<'a>, BoxedError>;

/// A type-erased `CheckBytes` function for the archived type of a trait impl.
#[doc(hidden)]
pub type CheckBytesDyn =
    unsafe fn(*const (), &mut dyn DynContext) -> Result<(), BoxedError>;

/// Selects the [`CheckBytesDyn`] function of a type that implements
/// `CheckBytes`.
///
/// `IsCheckBytesDyn::<T>::CHECK_BYTES_DYN` resolves to the inherent constant
/// if `T` implements `CheckBytes`, and to the constant of [`NotCheckBytesDyn`]
/// otherwise.
#[doc(hidden)]
pub struct IsCheckBytesDyn<T: ?Sized>(PhantomData<T>);

impl<T> IsCheckBytesDyn<T>
where
    T: for<'a> CheckBytes<DynCheckContext<'a>>,
{
    #[doc(hidden)]
    pub const CHECK_BYTES_DYN: CheckBytesDyn = Self::check_bytes_dyn;

    unsafe fn check_bytes_dyn(
        value: *const (),
        context: &mut dyn DynContext,
    ) -> Result<(), BoxedError> {
        let mut validator = DynValidator(context);
        // SAFETY: `value` points to a `T` because it was registered with
        // `T`'s trait object metadata.
        unsafe { T::check_bytes(value.cast(), Strategy::wrap(&mut validator)) }
    }
}

/// Selects a [`CheckBytesDyn`] function that always fails, for types that do
/// not implement `CheckBytes`. See [`IsCheckBytesDyn`].
#[doc(hidden)]
pub trait NotCheckBytesDyn {
    #[doc(hidden)]
    const CHECK_BYTES_DYN: CheckBytesDyn = check_bytes_dyn_unimplemented;
}

impl<T: ?Sized> NotCheckBytesDyn for T {}

#[derive(Debug)]
struct CheckBytesUnimplemented;

impl fmt::Display for CheckBytesUnimplemented {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the archived type of the trait object does not implement \
             CheckBytes"
        )
    }
}

impl Error for CheckBytesUnimplemented {}

unsafe fn check_bytes_dyn_unimplemented(
    _: *const (),
    _: &mut dyn DynContext,
) -> Result<(), BoxedError> {
    fail!(CheckBytesUnimplemented);
}

#[derive(Debug)]
struct UnregisteredImplId(ImplId);

impl fmt::Display for UnregisteredImplId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "impl ID {} is not registered", self.0)
    }
}

impl Error for UnregisteredImplId {}

// SAFETY: `verify` only returns `Ok` if the impl ID is registered for `T`,
// which is what `lookup_metadata` requires.
unsafe impl<T, C> Verify<C> for ArchivedDynMetadata<T>
where
    T: ?Sized + 'static,
    C: Fallible + ?Sized,
    C::Error: Source,
{
    fn verify(&self, _: &mut C) -> Result<(), C::Error> {
        let impl_id = self.impl_id();
        match TraitImpl::find::<T>(impl_id) {
            Some(_) => Ok(()),
            None => fail!(UnregisteredImplId(impl_id)),
        }
    }
}

#[derive(Debug)]
struct UnregisteredMetadata;

impl fmt::Display for UnregisteredMetadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "trait object metadata is not registered")
    }
}

impl Error for UnregisteredMetadata {}

/// Checks the archived value behind a trait object pointer.
///
/// This is called by the `CheckBytes` impls that
/// [`archive_dyn`](macro@crate::archive_dyn) generates for archived traits.
///
/// # Safety
///
/// `value` must be a pointer whose metadata was returned from
/// [`ArchivedDynMetadata::lookup_metadata`], and the data it points to must be
/// located in the subtree range of `context`.
pub unsafe fn check_trait_object<T, C>(
    value: *const T,
    context: &mut C,
) -> Result<(), C::Error>
where
    T: Pointee<Metadata = DynMetadata<T>> + ?Sized + 'static,
    C: Fallible + ArchiveContext + SharedContext + ?Sized,
    C::Error: Source,
{
    // SAFETY: All `DynMetadata<T>` have the same layout and validity.
    let metadata: DynMetadata<()> =
        unsafe { core::mem::transmute(ptr_meta::metadata(value)) };
    // `lookup_metadata` returns the exact metadata that was registered, so the
    // vtable addresses compare equal.
    let trait_impl = TraitImpls::iter().find(|trait_impl| {
        trait_impl.trait_type_id == TypeId::of::<T>()
            && trait_impl.metadata == metadata
    });
    let Some(trait_impl) = trait_impl else {
        fail!(UnregisteredMetadata);
    };
    let mut adapter = DynContextAdapter(context);
    // SAFETY: `value` points to a value of the type `trait_impl` was registered
    // for because it has the same metadata.
    unsafe { (trait_impl.check_bytes)(value.cast(), &mut adapter) }
        .map_err(<C::Error as Source>::new)
}
