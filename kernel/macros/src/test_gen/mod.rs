use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, ToTokens};
use syn::{
    parse::{Parse, Parser},
    token::Extern,
    Ident, ItemFn, LitStr,
};
use tiny_os_common::testing::TestConfig;

// TODO: add async?, run tests in separate QUEMU instances to catch error with out aborting operation/ run in threads

pub struct TestParser {
    funcs: Vec<Func>,
}

impl ToTokens for TestParser {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        //TODO this is ugly as fuck
        let mut test_funcs = Vec::new();
        let mut test_runners = Vec::new();
        for f in &self.funcs {
            let ident = &f.inner.sig.ident;
            if f.inner
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("test_case"))
            {
                let cfg_attrs: Vec<_> = f
                    .inner
                    .attrs
                    .iter()
                    .filter(|attr| attr.path().is_ident("cfg"))
                    .collect();

                test_funcs.push(quote! {
                    #(#cfg_attrs)*
                    #ident.run();
                });
            }
            if f.inner
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("runner"))
            {
                let cfg_attrs: Vec<_> = f
                    .inner
                    .attrs
                    .iter()
                    .filter(|attr| attr.path().is_ident("cfg"))
                    .collect();

                test_runners.push(quote! {
                    #(#cfg_attrs)*
                    #ident();
                });
            }
        }
        let funcs = &self.funcs;
        tokens.extend(quote! {
            // TODO: restrict visibility
            #[cfg(any(feature = "test_run", test))]
            pub mod tests {
                use super::*;
                use tiny_os_common::testing::TestCase;
                use os_macros::runner;
                #(#funcs)*
                // TODO: restrict visibility
                pub fn test_runner() {
                    #(#test_funcs)*
                    #(#test_runners)*
                }
            }
        });
    }
}

impl Parse for TestParser {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut funcs = Vec::new();
        while let Ok(r) = input.parse() {
            funcs.push(r);
        }
        Ok(Self { funcs })
    }
}

struct Func {
    inner: ItemFn,
}

impl ToTokens for Func {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let mut inner = self.inner.clone();
        inner
            .attrs
            .retain(|attr| !attr.path().is_ident("test_case"));
        tokens.extend(quote! {
            #inner
        })
    }
}

impl Parse for Func {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(Self {
            inner: input.parse()?,
        })
    }
}

struct CABIFunc {
    inner: ItemFn,
    name: Ident,
}

impl ToTokens for CABIFunc {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let inner = &self.inner;
        let name = &self.name;
        let inner_name = &inner.sig.ident;

        tokens.extend(quote! {
            #[os_macros::with_default_args]
            extern "C" fn #name() -> crate::kernel::threading::ProcessReturn {
                #inner
                #inner_name();
                0
            }
        });
    }
}

impl Parse for CABIFunc {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut inner: ItemFn = input.parse()?;
        let name = inner.sig.ident.clone();
        inner.sig.ident = format_ident!("{}_inner__", inner.sig.ident);
        // self_.inner.sig.abi = Some(syn::Abi {
        //     extern_token: Extern::default(),
        //     name: Some(LitStr::new("C", Span::call_site())),
        // });
        Ok(Self { inner, name })
    }
}

pub fn kernel_test_handler(
    attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let func: CABIFunc = syn::parse_macro_input!(input as CABIFunc);
    let attrs = syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated
        .parse(attr)
        .expect("malformed attrs");
    let config: TestConfigParser = attrs.into();
    let name = func.name.clone();
    let static_name = format_ident!("__STATIC_{}", name);
    let get_name_name = format_ident!("__GET_NAME_{}", name);

    quote! {
        #[cfg(feature = "test_run")]
        #func


        #[cfg(feature = "test_run")]
        #[allow(non_upper_case_globals)]
        const #get_name_name: &'static str = concat!(module_path!(), "::", stringify!(#name));


        #[cfg(feature = "test_run")]
        #[allow(non_upper_case_globals)]
        #[used]
        #[unsafe(link_section = ".tests")]
        pub static #static_name: crate::common::KernelTest = crate::common::KernelTest {
            name: tiny_os_common::testing::kernel::RawStr::from_s_str(#get_name_name),
            func: #name,
            config: #config
        };
    }
    .into()
}

#[derive(Default)]
struct TestConfigParser {
    inner: TestConfig,
}

impl From<syn::punctuated::Punctuated<syn::Meta, syn::Token![,]>> for TestConfigParser {
    fn from(value: syn::punctuated::Punctuated<syn::Meta, syn::Token![,]>) -> Self {
        let mut self_ = Self::default();
        if value
            .iter()
            .any(|attr| attr.path().is_ident("should_panic"))
        {
            self_.inner.should_panic = true;
        }
        if value.iter().any(|attr| attr.path().is_ident("verbose")) {
            self_.inner.verbose = true;
        }
        self_
    }
}

impl ToTokens for TestConfigParser {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let should_panic = self.inner.should_panic;
        let verbose = self.inner.verbose;
        let tokens_ = quote! {
            tiny_os_common::testing::TestConfig {
                should_panic: #should_panic,
                verbose: #verbose,
            }
        };
        tokens.extend(tokens_);
    }
}
