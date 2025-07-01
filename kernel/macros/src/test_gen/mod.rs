use std::{env, fs, path::Path};

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, ToTokens};
use serde::Deserialize;
use syn::{
    parse::{Parse, Parser},
    token::Extern,
    Expr, Ident, ItemFn, Lit, LitStr,
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
    // let owned_config = &config.inner;
    let name = func.name.clone();
    let static_name = format_ident!("__STATIC_{}", name);
    let get_name_name = format_ident!("__GET_NAME_{}", name);

    quote! {
        #[cfg(feature = "test_run")]
        #func


        #[cfg(feature = "test_run")]
        #[allow(non_upper_case_globals)]
        const #get_name_name: &'static str = concat!(module_path!(), "::", stringify!(#name));

        // this will generate all statics referenced by the TestConfig
        // #owned_config

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

// #[derive(Default, Deserialize)]
// struct OwnedTestConfig {
//     should_panic: bool,
//     verbose: bool,
//     devices: Vec<DeviceConfig>,
// }

// #[derive(Default, Deserialize)]
// struct DeviceConfig {
//     kind: String,
//     tags: Vec<String>,
// }

impl From<syn::punctuated::Punctuated<syn::Meta, syn::Token![,]>> for TestConfigParser {
    fn from(value: syn::punctuated::Punctuated<syn::Meta, syn::Token![,]>) -> Self {
        let mut self_ = Self::default();
        for attr in value.iter() {
            match attr {
                syn::Meta::Path(p) => match p {
                    p if p.is_ident("should_panic") => self_.inner.should_panic = true,
                    p if p.is_ident("verbose") => self_.inner.verbose = true,
                    _ => panic!("option not supported"),
                },
                syn::Meta::NameValue(v) => match &v.path {
                    p if p.is_ident("config") => {
                        let Expr::Lit(syn::ExprLit {
                            lit: Lit::Str(lit_str),
                            ..
                        }) = &v.value
                        else {
                            panic!("wrong value for config")
                        };
                        let config_str = if lit_str.value().ends_with(".toml") {
                            let parent_path =
                                env::var("CARGO_MANIFEST_DIR").expect("cargo manifest dir unset");
                            let path = Path::new(&parent_path).join(lit_str.value());
                            fs::read_to_string(path).expect("could not read config file")
                        } else {
                            lit_str.value()
                        };

                        // let config: OwnedTestConfig = toml::from_str(&config_str).unwrap();
                    }
                    _ => panic!("not supported"),
                },
                _ => panic!("not supported"),
            }
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

// impl ToTokens for OwnedTestConfig {
//     fn to_tokens(&self, tokens: &mut TokenStream) {
//         todo!()
//     }
// }
