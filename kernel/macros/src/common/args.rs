use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{
    Ident, ItemFn, LitInt, Type, parse::Parse, parse_macro_input, parse_quote, spanned::Spanned,
};

pub fn default_arg_parser(attr: TokenStream, input: TokenStream) -> TokenStream {
    let mut input_fn = parse_macro_input!(input as ItemFn);
    let args = parse_macro_input!(attr as NumArgs);
    let current = input_fn.sig.inputs.len();
    let (start, end) = if args.inner != 0 {
        (current, args.inner as usize + current)
    } else {
        (current, 6)
    };
    for i in start..end {
        let name = Ident::new(&format!("_arg{}", i), input_fn.sig.span());
        let arg_type: Type = syn::parse_str("crate::kernel::threading::task::Arg").unwrap();
        input_fn.sig.inputs.push(parse_quote!(#name: #arg_type));
    }

    quote! {#input_fn}.into()
}

struct NumArgs {
    inner: u8,
}

impl Parse for NumArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let num = input
            .parse::<LitInt>()
            .unwrap_or(LitInt::new("0", Span::call_site()));
        Ok(Self {
            inner: num.base10_parse().unwrap_or_default(),
        })
    }
}
