use quote::{quote, ToTokens};
use syn::{parse::Parse, ItemFn};

// TODO: add async?

pub struct TestParser {
    funcs: Vec<Func>,
}

impl ToTokens for TestParser {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let mut test_funcs = Vec::new();
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
        }
        let funcs = &self.funcs;
        tokens.extend(quote! {
            #[cfg(any(feature = "test_run", test))]
            mod tests {
                use super::*;
                #(#funcs)*

                pub(super) fn test_runner() {
                    #(#test_funcs)*
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
