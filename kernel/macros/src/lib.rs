#![allow(unused_doc_comments)]
mod mem;
mod test_gen;
use mem::addr::derive_addr;
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};
use test_gen::{kernel_test_handler, TestParser};

#[proc_macro]
pub fn tests(input: TokenStream) -> TokenStream {
    let out = parse_macro_input!(input as TestParser);
    quote! { #out }.into()
}
#[proc_macro_attribute]
pub fn runner(_attr: TokenStream, input: TokenStream) -> TokenStream {
    input
}

#[proc_macro_derive(Addr)]
pub fn addr(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    derive_addr(input)
}

#[proc_macro_attribute]
pub fn kernel_test(attr: TokenStream, input: TokenStream) -> TokenStream {
    /// transforms a
    /// #[kernel_test]
    /// fn test() {}
    /// to
    ///
    /// #[cfg(feature = "test_case")]
    /// extern "C" fn test() {}
    ///
    /// #[cfg(feature = "test_case")]
    /// #[used]
    /// #[unsafe(link_section = .tests)]
    /// pub static test: KernelTest = KernelTest { ... };
    kernel_test_handler(attr, input)
}

#[cfg(test)]
mod tests {
    use super::*;
}
