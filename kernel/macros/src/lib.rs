#![allow(unused_doc_comments)]
mod common;
mod mem;
mod test_gen;
use common::{
    args::default_arg_parser,
    fd_table::{CompositeTagAttrs, derive_composite_fd_tag, derive_fd_table},
};
use mem::addr::derive_addr;
use proc_macro::TokenStream;
use syn::{DeriveInput, ItemStruct, parse_macro_input};
use test_gen::kernel_test_handler;

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

#[proc_macro_attribute]
pub fn with_default_args(attr: TokenStream, input: TokenStream) -> TokenStream {
    default_arg_parser(attr, input)
}

#[proc_macro_derive(FDTable)]
pub fn fdtable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    derive_fd_table(input).into()
}

#[proc_macro_attribute]
pub fn fd_composite_tag(attr: TokenStream, input: TokenStream) -> TokenStream {
    let attrs = parse_macro_input!(attr as CompositeTagAttrs);
    let input = parse_macro_input!(input as ItemStruct);
    derive_composite_fd_tag(attrs, input).into()
}

#[cfg(test)]
mod tests {
    use super::*;
}
