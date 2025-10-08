use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote};
use syn::{
    Expr,
    ExprAssign,
    ExprLit,
    Ident,
    ItemFn,
    parse::{Parse, Parser},
    punctuated::Punctuated,
};
use tiny_os_common::testing::TestConfig;

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
    let config = TestConfigParser::parse(attrs, &name);
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
    should_open: Vec<(u32, String)>, // fd, path
}

impl TestConfigParser {
    fn parse(value: Punctuated<syn::Meta, syn::Token![,]>, _name: &Ident) -> Self {
        let mut self_ = Self::default();

        for attr in value.iter() {
            match attr {
                syn::Meta::Path(p) => match p {
                    p if p.is_ident("should_panic") => self_.inner.should_panic = true,
                    p if p.is_ident("silent") => {
                        self_.inner.verbose = false;
                        // set stderr, stdout to /kernel/null
                        self_.should_open.extend_from_slice(&[
                            (1, "/proc/kernel/null".into()),
                            (2, "/proc/kernel/null".into()),
                        ]);
                    }
                    p if p.is_ident("verbose") => {
                        self_.inner.verbose = true;
                        // set stdout and stderr to serial
                        // TODO fork also into fb
                        self_.should_open.extend_from_slice(&[
                            (1, "/proc/kernel/io/serial".into()),
                            (2, "/proc/kernel/io/serial".into()),
                        ]);
                    }
                    _ => panic!("option not supported"),
                },
                syn::Meta::NameValue(v) => match &v.path {
                    #[allow(unreachable_code)]
                    p if p.is_ident("config") => {
                        todo!();
                    }
                    p if p.is_ident("files") => {
                        let Expr::Array(syn::ExprArray { elems, .. }) = &v.value else {
                            panic!("wrong value for devices")
                        };
                        for elem in elems {
                            let Expr::Assign(ExprAssign { left, right, .. }) = elem else {
                                panic!("malformed file info");
                            };
                            let (
                                Expr::Lit(ExprLit {
                                    lit: syn::Lit::Int(fd),
                                    ..
                                }),
                                Expr::Lit(ExprLit {
                                    lit: syn::Lit::Str(path),
                                    ..
                                }),
                            ) = (left.as_ref(), right.as_ref())
                            else {
                                panic!("malformed file info");
                            };
                            self_
                                .should_open
                                .push((fd.base10_parse::<u32>().unwrap(), path.value()));
                        }
                    }
                    _ => panic!("arg not supported"),
                },
                _ => panic!("arg type not supported"),
            }
        }

        self_
    }
}

impl ToTokens for TestConfigParser {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let should_panic = self.inner.should_panic;
        let verbose = self.inner.verbose;
        let open_files = self.should_open.iter().map(|(fd, path)| {
            quote! { (#fd, #path) }
        });

        let tokens_: TokenStream = quote! {
            tiny_os_common::testing::TestConfig {
                should_panic: #should_panic,
                verbose: #verbose,
                open_files: &[#(#open_files), *],
            }
        };
        tokens.extend(tokens_);
    }
}
