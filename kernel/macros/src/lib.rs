mod test_gen;

use proc_macro::TokenStream;
use quote::quote;
use syn::parse_macro_input;
use test_gen::TestParser;

#[proc_macro]
pub fn tests(input: TokenStream) -> TokenStream {
    let out = parse_macro_input!(input as TestParser);
    quote! { #out }.into()
}

#[cfg(test)]
mod tests {
    use super::*;
}
