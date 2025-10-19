pub mod data_structures;
pub mod sync;

#[macro_export]
macro_rules! impl_dgb {
    (@impl [$($impl_generics:tt)*] $name:ty => $msg:expr) => {
        impl<$($impl_generics)*> ::core::fmt::Debug for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str($msg)
            }
        }
    };

    ($name:ty) => {
        impl_dgb!(@impl [] $name => "");
    };

    ($name:ty where [$($generics:tt)*]) => {
        impl_dgb!(@impl [$($generics)*] $name => "");
    };

    ($name:ty => $msg:expr) => {
        impl_dgb!(@impl [] $name => $msg);
    };

    ($name:ty where [$($generics:tt)*] => $msg:expr) => {
        impl_dgb!(@impl [$($generics)*] $name => $msg);
    };
}
