//! Procedural macros for BGM.
//!
//! Currently provides [`derive@Interpolate`], a derive macro that wires a
//! struct into BGM's `{{ variable }}` template substitution. The heavy lifting
//! lives in `bgm_core::Interpolate`; this macro only generates the recursive
//! field-by-field call, so any field type that itself implements `Interpolate`
//! (`String`, `Option<T>`, `Vec<T>`, nested structs…) just works.

use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, parse_macro_input};

/// Derives `bgm_core::Interpolate` for a named-field struct.
///
/// The generated impl rebuilds the struct, calling `interpolate` on every
/// field. This is the classic *Derive* pattern: boilerplate is generated once,
/// the recursive contract is defined in the trait.
///
/// ```ignore
/// use bgm_macros::Interpolate;
///
/// #[derive(Interpolate)]
/// struct Request {
///     url: String,
///     body: Option<String>,
/// }
/// ```
#[proc_macro_derive(Interpolate)]
pub fn derive_interpolate(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let body = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => {
                let recurse = fields.named.iter().map(|f| {
                    let field = f.ident.as_ref().expect("named field");
                    quote! {
                        #field: ::bgm_core::Interpolate::interpolate(&self.#field, ctx)
                    }
                });
                quote! { Self { #(#recurse),* } }
            }
            Fields::Unit => quote! { Self },
            Fields::Unnamed(_) => {
                return syn::Error::new_spanned(
                    name,
                    "#[derive(Interpolate)] supports structs with named fields or unit structs",
                )
                .to_compile_error()
                .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(
                name,
                "#[derive(Interpolate)] can only be applied to structs",
            )
            .to_compile_error()
            .into();
        }
    };

    quote! {
        impl #impl_generics ::bgm_core::Interpolate for #name #ty_generics #where_clause {
            fn interpolate(&self, ctx: &dyn ::bgm_core::Lookup) -> Self {
                #body
            }
        }
    }
    .into()
}
