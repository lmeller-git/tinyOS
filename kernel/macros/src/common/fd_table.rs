use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    parse::Parse, punctuated::Punctuated, Data, DataEnum, DeriveInput, Ident, ItemStruct, Token,
};

pub fn derive_fd_table(input: DeriveInput) -> TokenStream {
    let Data::Enum(DataEnum { variants, .. }) = &input.data else {
        panic!("derive FDTable only defined for enums");
    };
    let generated = variants.iter().map(|variant| {
        let var_ident = &variant.ident;
        let tag_ident = syn::Ident::new(&format!("{}Tag", var_ident), var_ident.span());
        quote! {
            pub trait #var_ident {}
            #[derive(Clone, Copy, PartialEq, Eq, Debug)]
            pub struct #tag_ident;
            impl #var_ident for #tag_ident {}
            impl FdTag for #tag_ident {}

            impl Attacheable for FdEntry<#tag_ident> {
                fn attach_to(self, devices: &mut TaskDevices) {
                    let v = devices.get_mut(FdEntryType::#var_ident);
                    if let Some(inner) = v.as_mut() {
                        inner.add(self.inner);
                    } else {
                        *v = Some(self.inner);
                    }
                }
            }
        }
    });
    quote! {
        #(#generated)*
    }
}

pub fn derive_composite_fd_tag(attr: CompositeTagAttrs, input: ItemStruct) -> TokenStream {
    let struct_name = &input.ident;

    let trait_bounds = attr.traits.iter().map(|trait_bound| {
        quote! {
            impl #trait_bound for #struct_name {}
        }
    });

    let attach_impls = attr.traits.iter().map(|trait_bound| {
        let tag_name = syn::Ident::new(&format!("{}Tag", trait_bound), trait_bound.span());
        quote! {
            let variant: FdEntry<#tag_name> = ::core::convert::From::<FdEntry<#struct_name>>::from(self.clone());
            Attacheable::attach_to(variant, devices);
        }
    });

    let attacheable = quote! {
        impl CompositeAttacheable for FdEntry<#struct_name> {
            fn attach_all(self, devices: &mut TaskDevices) {
                #(#attach_impls)*
            }
        }

        impl Attacheable for FdEntry<#struct_name> {
            fn attach_to(self, devices: &mut TaskDevices) {
                self.attach_all(devices)
            }
        }
    };

    let from_impls = attr.traits.iter().map(|trait_bound| {
        let tag_name = syn::Ident::new(&format!("{}Tag", trait_bound), trait_bound.span());
        quote! {
            impl From<FdEntry<#struct_name>> for FdEntry<#tag_name> {
                fn from(value: FdEntry<#struct_name>) -> Self {
                    FdEntry {
                        inner: value.inner,
                        _phantom_type: ::core::marker::PhantomData::<#tag_name>
                    }
                }
            }
        }
    });

    quote! {
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        #input
        impl FdTag for #struct_name {}
        #(#trait_bounds)*
        #(#from_impls)*
        #attacheable

    }
}

pub struct CompositeTagAttrs {
    traits: Vec<Ident>,
}

impl Parse for CompositeTagAttrs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let traits = Punctuated::<Ident, Token![,]>::parse_terminated(input)?
            .into_iter()
            .collect();
        Ok(Self { traits })
    }
}
