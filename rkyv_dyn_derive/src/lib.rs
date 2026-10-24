//! Procedural macros for `rkyv_dyn`.

#![deny(
    rustdoc::broken_intra_doc_links,
    missing_docs,
    rustdoc::missing_crate_level_docs
)]

extern crate proc_macro;

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    parse::{Parse, ParseStream, Result},
    parse_macro_input, parse_quote, Attribute, Error, GenericArgument, Ident,
    ItemImpl, ItemTrait, LitStr, Path, PathArguments, Token, Type, TypePath,
    Visibility,
};

enum Input {
    Impl(ItemImpl),
    Trait(ItemTrait),
}

impl Parse for Input {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut attrs = Attribute::parse_outer(input)?;

        let ahead = input.fork();
        ahead.parse::<Visibility>()?;
        ahead.parse::<Option<Token![unsafe]>>()?;

        if ahead.peek(Token![trait]) {
            let mut item: ItemTrait = input.parse()?;
            attrs.extend(item.attrs);
            item.attrs = attrs;
            Ok(Input::Trait(item))
        } else if ahead.peek(Token![impl]) {
            let mut item: ItemImpl = input.parse()?;
            if item.trait_.is_none() {
                let impl_token = item.impl_token;
                let ty = item.self_ty;
                let span = quote!(#impl_token #ty);
                let msg = "expected impl Trait for Type";
                return Err(Error::new_spanned(span, msg));
            }
            attrs.extend(item.attrs);
            item.attrs = attrs;
            Ok(Input::Impl(item))
        } else {
            Err(input.error("expected trait or impl block"))
        }
    }
}

struct Args {
    serialize: Option<LitStr>,
    deserialize: Option<Option<LitStr>>,
    archived: Option<LitStr>,
}

impl Parse for Args {
    fn parse(input: ParseStream) -> Result<Self> {
        mod kw {
            syn::custom_keyword!(serialize);
            syn::custom_keyword!(deserialize);
            syn::custom_keyword!(archived);
        }

        let mut serialize = None;
        let mut deserialize = None;
        let mut archived = None;

        let mut needs_punct = false;
        while !input.is_empty() {
            if needs_punct {
                input.parse::<Token![,]>()?;
            }

            if input.peek(kw::serialize) {
                if serialize.is_some() {
                    return Err(input.error("duplicate serialize argument"));
                }

                input.parse::<kw::serialize>()?;
                input.parse::<Token![=]>()?;
                serialize = Some(input.parse::<LitStr>()?);
            } else if input.peek(kw::deserialize) {
                if deserialize.is_some() {
                    return Err(input.error("duplicate deserialize argument"));
                }

                input.parse::<kw::deserialize>()?;
                if input.peek(Token![=]) {
                    input.parse::<Token![=]>()?;
                    deserialize = Some(Some(input.parse::<LitStr>()?));
                } else {
                    deserialize = Some(None);
                }
            } else if input.peek(kw::archived) {
                if archived.is_some() {
                    return Err(input.error("duplicate archived argument"));
                }

                input.parse::<kw::archived>()?;
                input.parse::<Token![=]>()?;
                archived = Some(input.parse::<LitStr>()?);
            } else {
                return Err(input.error(
                    "expected serialize = \"...\", deserialize = \"...\" or \
                     archived = \"...\" parameters",
                ));
            }

            needs_punct = true;
        }

        Ok(Args {
            serialize,
            deserialize,
            archived,
        })
    }
}

impl Args {
    /// Returns the name of the generated serialize trait.
    fn serialize_name(&self, name: &Ident) -> Ident {
        match &self.serialize {
            Some(ser_name) => Ident::new(&ser_name.value(), ser_name.span()),
            None => Ident::new(&format!("Serialize{name}"), name.span()),
        }
    }

    /// Returns the name of the generated archived trait.
    fn archived_name(&self, name: &Ident) -> Ident {
        match &self.deserialize {
            Some(Some(de_name)) => Ident::new(&de_name.value(), de_name.span()),
            Some(None) => {
                Ident::new(&format!("Deserialize{name}"), name.span())
            }
            None => Ident::new(&format!("Archived{name}"), name.span()),
        }
    }
}

/// Creates archivable trait objects.
///
/// Prepend to trait definitions and implementations.
///
/// On a trait `Trait`, this generates:
///
/// - A serialize trait (by default `SerializeTrait`), which is implemented for
///   every type that implements `Trait` and can be serialized. Box this trait
///   to serialize trait objects.
/// - An archived trait, which archived trait objects are boxed as. By default
///   it is named `ArchivedTrait`, or `DeserializeTrait` with `deserialize`.
///
/// The serialize trait has an extra generic parameter for the serializer error
/// type. With `deserialize`, both traits have extra generic parameters for the
/// serializer and deserializer error types.
///
/// On an implementation `impl Trait for Type`, this generates
/// `DeserializeDyn` for the archived type of `Type` when `deserialize` is
/// passed, and does nothing otherwise. It must have the same arguments as the
/// trait invocation, plus `archived` if needed.
///
/// Implementations are not registered automatically. Register every archived
/// type with the archived trait it implements with `register_trait_impls!`.
///
/// # Parameters
///
/// - `serialize = "..."`: Chooses the name of the serialize trait. By default,
///   it will be named "Serialize" + your trait name.
/// - `deserialize`, `deserialize = "..."`: Adds deserialization support to the
///   archived trait. You can choose the name of the archived trait and by
///   default it will be named "Deserialize" + your trait name.
/// - `archived = "..."`: On an impl, names the archived type of `Type`. By
///   default it is "Archived" + the type name, which is what rkyv derives
///   unless `#[rkyv(archived = ...)]` is given.
#[proc_macro_attribute]
pub fn archive_dyn(
    attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let input = parse_macro_input!(item as Input);
    let args = parse_macro_input!(attr as Args);

    let result = match &input {
        Input::Impl(input) => generate_impl(input, &args),
        Input::Trait(input) => generate_traits(input, &args),
    };

    match result {
        Ok(result) => result.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Returns `path` with its last segment renamed to `name` and `extra_args`
/// appended to its generic arguments.
fn rename_path(
    path: &Path,
    name: Ident,
    extra_args: &[GenericArgument],
) -> Path {
    let mut path = path.clone();
    let last = path.segments.last_mut().unwrap();
    last.ident = name;
    if !extra_args.is_empty() {
        if let PathArguments::None = last.arguments {
            last.arguments = PathArguments::AngleBracketed(parse_quote!(<>));
        }
        if let PathArguments::AngleBracketed(args) = &mut last.arguments {
            args.args.extend(extra_args.iter().cloned());
        }
    }
    path
}

fn generate_impl(input: &ItemImpl, args: &Args) -> Result<TokenStream> {
    if args.deserialize.is_none() {
        return Ok(quote! { #input });
    }
    let (trait_, _) = input.trait_.as_ref().unwrap();
    let trait_name = &trait_.segments.last().unwrap().ident;

    let ty = &input.self_ty;
    let archived = archived_type(ty, args)?;
    let se: Ident = parse_quote!(__SE);
    let de: Ident = parse_quote!(__DE);
    let error_args: [GenericArgument; 2] =
        [parse_quote!(#se), parse_quote!(#de)];

    let ser_trait =
        rename_path(trait_, args.serialize_name(trait_name), &error_args);

    let mut generics = input.generics.clone();
    generics.params.push(parse_quote!(#se));
    generics.params.push(parse_quote!(#de));
    let where_clause = generics.make_where_clause();
    where_clause.predicates.push(parse_quote! {
        #ty: ::rkyv::Archive<Archived = #archived>
    });
    where_clause.predicates.push(parse_quote! {
        #archived: for<'__a> ::rkyv::Deserialize<
            #ty,
            dyn ::rkyv_dyn::DynDeserializer<#de> + '__a,
        >
    });
    where_clause.predicates.push(parse_quote! {
        #ty: #ser_trait
    });
    let (impl_generics, _, where_clause) = generics.split_for_impl();

    // The impl is for the archived type itself rather than `Archived<#ty>`.
    // rustc considers an impl for an associated type in another crate to
    // overlap with every impl of the same trait, so impls in different crates
    // would conflict.
    Ok(quote! {
        #input

        impl #impl_generics ::rkyv_dyn::DeserializeDyn<dyn #ser_trait, #de>
            for #archived
        #where_clause
        {
            fn deserialize_dyn(
                &self,
                deserializer: &mut dyn ::rkyv_dyn::DynDeserializer<#de>,
                out: *mut dyn #ser_trait,
            ) -> ::core::result::Result<(), #de> {
                // SAFETY: `out` points to memory allocated for the layout of
                // `#ty` because its metadata is the metadata of `#ty`.
                unsafe {
                    <
                        Self as ::rkyv::DeserializeUnsized<#ty, _>
                    >::deserialize_unsized(self, deserializer, out.cast())
                }
            }

            fn deserialized_pointer_metadata(
                &self,
            ) -> ::rkyv::ptr_meta::DynMetadata<dyn #ser_trait> {
                ::rkyv::ptr_meta::metadata(
                    ::core::ptr::null::<#ty>() as *const dyn #ser_trait
                )
            }
        }
    })
}

/// Returns the archived type of `ty`: the `archived` argument if it is given,
/// and otherwise the default name that rkyv derives, "Archived" + the type
/// name.
fn archived_type(ty: &Type, args: &Args) -> Result<Type> {
    if let Some(archived) = &args.archived {
        return archived.parse();
    }
    let mut archived = ty.clone();
    let Type::Path(TypePath {
        qself: None, path, ..
    }) = &mut archived
    else {
        return Err(Error::new_spanned(
            ty,
            "expected a type path, pass archived = \"...\" for other types",
        ));
    };
    let last = path.segments.last_mut().unwrap();
    last.ident =
        Ident::new(&format!("Archived{}", last.ident), last.ident.span());
    Ok(archived)
}

fn generate_traits(input: &ItemTrait, args: &Args) -> Result<TokenStream> {
    if let Some(archived) = &args.archived {
        return Err(Error::new_spanned(
            archived,
            "archived can only be passed to impls",
        ));
    }
    let vis = &input.vis;
    let name = &input.ident;
    let ser_trait = args.serialize_name(name);
    let ar_trait = args.archived_name(name);

    let se = Ident::new("__SE", Span::call_site());
    let de = Ident::new("__DE", Span::call_site());

    let params = input.generics.params.iter().collect::<Vec<_>>();
    let args_ = input.generics.params.iter().map(|p| match p {
        syn::GenericParam::Type(p) => {
            let ident = &p.ident;
            quote! { #ident }
        }
        syn::GenericParam::Lifetime(p) => {
            let lifetime = &p.lifetime;
            quote! { #lifetime }
        }
        syn::GenericParam::Const(p) => {
            let ident = &p.ident;
            quote! { #ident }
        }
    });
    let args_ = args_.collect::<Vec<_>>();
    let where_clause = &input.generics.where_clause;
    let where_predicates = where_clause
        .as_ref()
        .map(|w| {
            let predicates = w.predicates.iter();
            quote! { #(#predicates,)* }
        })
        .unwrap_or_default();

    // The user's trait parameters followed by the error type parameters.
    let (error_params, ar_error_params) = if args.deserialize.is_some() {
        (quote! { #se, #de }, quote! { #se, #de })
    } else {
        (quote! { #se }, quote! {})
    };
    // The serialize trait with the serializer error type replaced by the
    // error type of the serializer `__S`.
    let (ser_unsized_params, ser_unsized_args) = if args.deserialize.is_some() {
        (
            quote! { __S, #(#params,)* #de },
            quote! { #(#args_,)* __S::Error, #de },
        )
    } else {
        (
            quote! { __S, #(#params,)* },
            quote! { #(#args_,)* __S::Error },
        )
    };
    // The archived trait of `dyn #ser_trait<#ser_unsized_args>`.
    let ser_unsized_ar_args = if args.deserialize.is_some() {
        quote! { #(#args_,)* __S::Error, #de }
    } else {
        quote! { #(#args_,)* }
    };
    let ser_params = quote! { #(#params,)* #error_params };
    let ser_args = quote! { #(#args_,)* #error_params };
    let ar_params = quote! { #(#params,)* #ar_error_params };
    let ar_args = quote! { #(#args_,)* #ar_error_params };
    let trait_args = quote! { #(#args_),* };

    let (ar_supertraits, ar_impl_bounds, de_impl) =
        if args.deserialize.is_some() {
            (
                quote! {
                    + ::rkyv_dyn::DeserializeDyn<dyn #ser_trait<#ser_args>, #de>
                },
                quote! {
                    __T: ::rkyv_dyn::DeserializeDyn<
                        dyn #ser_trait<#ser_args>,
                        #de,
                    >,
                },
                quote! {
                    impl<__D, #(#params,)* #se>
                        ::rkyv::DeserializeUnsized<
                            dyn #ser_trait<#(#args_,)* #se, __D::Error>,
                            __D,
                        >
                        for dyn #ar_trait<#(#args_,)* #se, __D::Error>
                    where
                        __D: ::rkyv::rancor::Fallible
                            + ::rkyv_dyn::AsDynDeserializer<__D::Error>
                            + ?Sized,
                        dyn #ar_trait<#(#args_,)* #se, __D::Error>: 'static,
                        #where_predicates
                    {
                        unsafe fn deserialize_unsized(
                            &self,
                            deserializer: &mut __D,
                            out: *mut dyn #ser_trait<
                                #(#args_,)* #se, __D::Error
                            >,
                        ) -> ::core::result::Result<(), __D::Error> {
                            self.deserialize_dyn(
                                deserializer.as_dyn_deserializer(),
                                out,
                            )
                        }

                        fn deserialize_metadata(
                            &self,
                        ) -> ::rkyv::ptr_meta::DynMetadata<
                            dyn #ser_trait<#(#args_,)* #se, __D::Error>
                        > {
                            self.deserialized_pointer_metadata()
                        }
                    }
                },
            )
        } else {
            (quote! {}, quote! {}, quote! {})
        };

    #[cfg(feature = "bytecheck")]
    let validation_impl = quote! {
        unsafe impl<__C, #ar_params> ::rkyv::bytecheck::CheckBytes<__C>
            for dyn #ar_trait<#ar_args>
        where
            __C: ::rkyv::rancor::Fallible
                + ::rkyv::validation::ArchiveContext
                + ::rkyv::validation::SharedContext
                + ?Sized,
            __C::Error: ::rkyv::rancor::Source,
            dyn #ar_trait<#ar_args>: 'static,
            #where_predicates
        {
            unsafe fn check_bytes(
                value: *const Self,
                context: &mut __C,
            ) -> ::core::result::Result<(), __C::Error> {
                // SAFETY: The caller has guaranteed that `value` is a pointer
                // to an archived trait object in the subtree range of
                // `context`, and the metadata of archived trait objects is
                // always looked up from the registered trait impls.
                unsafe {
                    ::rkyv_dyn::validation::check_trait_object(value, context)
                }
            }
        }
    };

    #[cfg(not(feature = "bytecheck"))]
    let validation_impl = quote! {};

    Ok(quote! {
        #input

        #[::rkyv::ptr_meta::pointee(crate = ::rkyv::ptr_meta)]
        #vis trait #ser_trait<#ser_params>:
            #name<#trait_args> + ::rkyv_dyn::SerializeDyn<#se>
        #where_clause
        {
            #[doc(hidden)]
            fn archived_impl_id(
                &self,
            ) -> ::core::result::Result<
                ::rkyv_dyn::ImplId,
                ::rkyv_dyn::UnregisteredImpl,
            >;
        }

        #[::rkyv::ptr_meta::pointee(crate = ::rkyv::ptr_meta)]
        #vis trait #ar_trait<#ar_params>:
            #name<#trait_args> + ::rkyv::Portable #ar_supertraits
        #where_clause
        {
        }

        const _: () = {
            impl<__T, #ser_params> #ser_trait<#ser_args> for __T
            where
                __T: #name<#trait_args>
                    + for<'__a> ::rkyv::Serialize<
                        dyn ::rkyv_dyn::DynSerializer<#se> + '__a
                    >,
                __T::Archived: 'static,
                dyn #ar_trait<#ar_args>: 'static,
                #where_predicates
            {
                fn archived_impl_id(
                    &self,
                ) -> ::core::result::Result<
                    ::rkyv_dyn::ImplId,
                    ::rkyv_dyn::UnregisteredImpl,
                > {
                    ::rkyv_dyn::TraitImpl::find_by_type::<
                        __T::Archived,
                        dyn #ar_trait<#ar_args>,
                    >()
                    .map(::rkyv_dyn::TraitImpl::impl_id)
                }
            }

            impl<#ser_params> ::rkyv::ArchiveUnsized
                for dyn #ser_trait<#ser_args>
            where
                dyn #ar_trait<#ar_args>: 'static,
                #where_predicates
            {
                type Archived = dyn #ar_trait<#ar_args>;

                fn archived_metadata(
                    &self,
                ) -> ::rkyv::ArchivedMetadata<Self> {
                    // `serialize_unsized` fails if the impl is not
                    // registered, so this only panics if it was not called.
                    let impl_id = self
                        .archived_impl_id()
                        .unwrap_or_else(|error| panic!("{error}"));
                    ::rkyv_dyn::ArchivedDynMetadata::new(impl_id)
                }
            }

            // SAFETY: `layout_raw` returns the layout from the vtable, which
            // is always valid.
            impl<#ser_params> ::rkyv::traits::LayoutRaw
                for dyn #ser_trait<#ser_args>
            where
                #where_predicates
            {
                fn layout_raw(
                    metadata: ::rkyv::ptr_meta::DynMetadata<Self>,
                ) -> ::core::result::Result<
                    ::core::alloc::Layout,
                    ::core::alloc::LayoutError,
                > {
                    Ok(metadata.layout())
                }
            }

            impl<#ser_unsized_params> ::rkyv::SerializeUnsized<__S>
                for dyn #ser_trait<#ser_unsized_args>
            where
                __S: ::rkyv::rancor::Fallible
                    + ::rkyv_dyn::AsDynSerializer<__S::Error>
                    + ?Sized,
                __S::Error: ::rkyv::rancor::Source,
                dyn #ar_trait<#ser_unsized_ar_args>: 'static,
                #where_predicates
            {
                fn serialize_unsized(
                    &self,
                    serializer: &mut __S,
                ) -> ::core::result::Result<usize, __S::Error> {
                    self.archived_impl_id()
                        .map_err(<__S::Error as ::rkyv::rancor::Source>::new)?;
                    self.serialize_dyn(serializer.as_dyn_serializer())
                }
            }

            impl<#ar_params> ::rkyv::traits::ArchivePointee
                for dyn #ar_trait<#ar_args>
            where
                dyn #ar_trait<#ar_args>: 'static,
                #where_predicates
            {
                type ArchivedMetadata =
                    ::rkyv_dyn::ArchivedDynMetadata<Self>;

                fn pointer_metadata(
                    archived: &Self::ArchivedMetadata,
                ) -> ::rkyv::ptr_meta::DynMetadata<Self> {
                    archived.lookup_metadata()
                }
            }

            // SAFETY: `layout_raw` returns the layout from the vtable, which
            // is always valid.
            impl<#ar_params> ::rkyv::traits::LayoutRaw
                for dyn #ar_trait<#ar_args>
            where
                #where_predicates
            {
                fn layout_raw(
                    metadata: ::rkyv::ptr_meta::DynMetadata<Self>,
                ) -> ::core::result::Result<
                    ::core::alloc::Layout,
                    ::core::alloc::LayoutError,
                > {
                    Ok(metadata.layout())
                }
            }

            impl<__T, #ar_params> #ar_trait<#ar_args> for __T
            where
                __T: #name<#trait_args> + ::rkyv::Portable,
                #ar_impl_bounds
                #where_predicates
            {
            }

            #de_impl

            #validation_impl
        };
    })
}
