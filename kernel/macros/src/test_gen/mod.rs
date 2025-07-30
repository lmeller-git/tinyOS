use std::{env, fs, path::Path};

use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote};
use syn::{
    Expr, Ident, ItemFn, Lit, PathSegment,
    parse::{Parse, Parser},
    punctuated::Punctuated,
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
    let name = func.name.clone();
    let (config, tokens) = TestConfigParser::parse(attrs, &name);
    let static_name = format_ident!("__STATIC_{}", name);
    let get_name_name = format_ident!("__GET_NAME_{}", name);

    quote! {
        #[cfg(feature = "test_run")]
        #tokens

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
    device_inits: Vec<Ident>,
}

impl TestConfigParser {
    fn parse(value: Punctuated<syn::Meta, syn::Token![,]>, name: &Ident) -> (Self, TokenStream) {
        let mut self_ = Self::default();
        let mut funcs: Vec<TokenStream> = Vec::new();
        for attr in value.iter() {
            match attr {
                syn::Meta::Path(p) => match p {
                    p if p.is_ident("should_panic") => self_.inner.should_panic = true,
                    p if p.is_ident("verbose") => {
                        self_.inner.verbose = true;
                        self_.configure_device(&get_verbose_config(), &mut funcs, name);
                    }
                    _ => panic!("option not supported"),
                },
                syn::Meta::NameValue(v) => match &v.path {
                    #[allow(unreachable_code)]
                    p if p.is_ident("config") => {
                        todo!();
                        let Expr::Lit(syn::ExprLit {
                            lit: Lit::Str(lit_str),
                            ..
                        }) = &v.value
                        else {
                            panic!("wrong value for config")
                        };
                        let _config_str = if lit_str.value().ends_with(".toml") {
                            let parent_path =
                                env::var("CARGO_MANIFEST_DIR").expect("cargo manifest dir unset");
                            let path = Path::new(&parent_path).join(lit_str.value());
                            fs::read_to_string(path).expect("could not read config file")
                        } else {
                            lit_str.value()
                        };

                        // let config: OwnedTestConfig = toml::from_str(&config_str).unwrap();
                    }
                    p if p.is_ident("devices") => {
                        let Expr::Array(syn::ExprArray { elems, .. }) = &v.value else {
                            panic!("wrong value for devices")
                        };
                        for elem in elems {
                            self_.configure_device(elem, &mut funcs, name);
                        }
                    }
                    _ => panic!("arg not supported"),
                },
                _ => panic!("arg type not supported"),
            }
        }

        (
            self_,
            quote! {
                #(#funcs)*
            },
        )
    }

    fn configure_device(&mut self, device: &Expr, acc: &mut Vec<TokenStream>, name: &Ident) {
        let Expr::Call(syn::ExprCall { func, args, .. }) = device else {
            panic!("wrong value ffor device")
        };

        let device_name = if let Expr::Path(path) = func.as_ref() {
            &path.path
        } else {
            panic!("wrong syntax")
        };

        let tag = if let Some(Expr::Path(path)) = args.first() {
            &path.path
        } else {
            panic!("wrong syntax")
        };

        let (fn_name, device_builder) = match device_name {
            device if device.is_ident("serial") => {
                let fn_name = format_ident!("configure_serial_for_{}", name);
                (
                    fn_name,
                    quote! {
                        let device: crate::kernel::devices::FdEntry<#tag> = crate::kernel::devices::DeviceBuilder::tty().serial();
                    },
                )
            }
            device if device.is_ident("framebuffer") => {
                let fn_name = format_ident!("configure_fb_for_{}", name);
                (
                    fn_name,
                    quote! {
                        let device: crate::kernel::devices::FdEntry<#tag> = crate::kernel::devices::DeviceBuilder::tty().fb();
                    },
                )
            }

            device if device.is_ident("keyboard") => {
                let fn_name = format_ident!("configure_keyboard_for_{}", name);
                (
                    fn_name,
                    quote! {
                        let device: crate::kernel::devices::FdEntry<#tag> = crate::kernel::devices::DeviceBuilder::tty().keyboard();
                    },
                )
            }
            _ => panic!("device not supported"),
        };
        acc.push(quote! {
            fn #fn_name(devices: *mut ()) {
                let mut devices = unsafe { &mut *(devices as *mut crate::kernel::devices::TaskDevices)};
                #device_builder
                devices.attach(device);
            }
        });
        self.device_inits.push(fn_name);
    }
}

fn get_verbose_config() -> Expr {
    Expr::Call(syn::ExprCall {
        func: Box::new(Expr::Path(syn::ExprPath {
            attrs: Vec::new(),
            qself: None,
            path: syn::Path {
                leading_colon: None,
                segments: [PathSegment {
                    ident: format_ident!("serial"),
                    arguments: syn::PathArguments::None,
                }]
                .into_iter()
                .collect(),
            },
        })),
        args: [Expr::Path(syn::ExprPath {
            attrs: Vec::new(),
            qself: None,
            path: syn::Path {
                leading_colon: None,
                segments: ["crate", "kernel", "devices", "SuccessSinkTag"]
                    .into_iter()
                    .map(|seg| PathSegment {
                        ident: format_ident!("{seg}"),
                        arguments: syn::PathArguments::None,
                    })
                    .collect(),
            },
        })]
        .into_iter()
        .collect(),
        attrs: Vec::new(),
        paren_token: syn::token::Paren::default(),
    })
}

impl ToTokens for TestConfigParser {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let should_panic = self.inner.should_panic;
        let verbose = self.inner.verbose;
        let inits = &self.device_inits;
        let tokens_ = quote! {
            tiny_os_common::testing::TestConfig {
                should_panic: #should_panic,
                verbose: #verbose,
                device_inits: &[#(#inits),*]
            }
        };
        tokens.extend(tokens_);
    }
}
