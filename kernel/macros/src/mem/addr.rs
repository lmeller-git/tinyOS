use proc_macro::TokenStream;
use quote::quote;
use syn::DeriveInput;

pub fn derive_addr(input: DeriveInput) -> TokenStream {
    let name = &input.ident;

    quote! {
        use core::{
            fmt::Display,
            ops::{Add, Shl, ShlAssign, Shr, ShrAssign, Sub}
        };

        impl Addr for #name {
            fn into_inner(self) -> u64 {
                self.inner
            }

            fn new(addr: u64) -> Self {
                Self { inner: addr }
            }
        }

        impl From<u64> for #name {
            fn from(addr: u64) -> Self {
                Self { inner: addr }
            }
        }

        impl From<#name> for u64 {
            fn from(addr: #name) -> u64 {
                addr.inner
            }
        }

        impl Add<u64> for #name {
            type Output = Self;
            fn add(self, rhs: u64) -> Self::Output {
                Self { inner: self.inner + rhs }
            }
        }

        impl Sub<u64> for #name {
            type Output = Self;
            fn sub(self, rhs: u64) -> Self::Output {
                Self { inner: self.inner - rhs }
            }
        }

        impl Shl<usize> for #name {
            type Output = Self;
            fn shl(self, rhs: usize) -> Self::Output {
                Self { inner: self.inner << rhs }
            }
        }

        impl Shr<usize> for #name {
            type Output = Self;
            fn shr(self, rhs: usize) -> Self::Output {
                Self { inner: self.inner >> rhs }
            }
        }

        impl ShlAssign<usize> for #name {
            fn shl_assign(&mut self, rhs: usize) {
                self.inner <<= rhs;
            }
        }

        impl ShrAssign<usize> for #name {
            fn shr_assign(&mut self, rhs: usize) {
                self.inner >>= rhs;
            }
        }

        impl Default for #name {
            fn default() -> Self {
                Self { inner: 0 }
            }
        }

        impl Display for #name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                writeln!(f, "{}", self.inner)?;
                Ok(())
            }
        }

    }
    .into()
}
