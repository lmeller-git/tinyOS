#![allow(unexpected_cfgs)]
#![allow(dead_code)]
#![allow(unused_imports)]

use std::sync::atomic::{AtomicBool, AtomicI16, Ordering};

use os_macros::{kernel_test, tests};
use tiny_os_common::testing::TestCase;

static A: AtomicI16 = AtomicI16::new(0);
static B: AtomicBool = AtomicBool::new(false);
static C: AtomicI16 = AtomicI16::new(0);

#[cfg(test)]
tests! {
    #[test_case]
    fn a() {
        A.store(42, Ordering::Relaxed);
    }

    fn b() {
        B.store(true, Ordering::Relaxed);
    }

    #[test_case]
    fn c() {
        C.store(42, Ordering::Relaxed);
    }

    #[cfg(any())]
    #[test_case]
    fn not_built() {
        assert!(true);
    }

    #[cfg(test)]
    #[test_case]
    #[test]
    fn test_test() {

    }

}

#[cfg(test)]
#[test]
fn correct_calls() {
    A.store(0, Ordering::Relaxed);
    B.store(false, Ordering::Relaxed);
    C.store(0, Ordering::Relaxed);
    tests::test_runner();
    assert_eq!(A.load(Ordering::Relaxed), 42);
    assert!(!B.load(Ordering::Relaxed));
    assert_eq!(C.load(Ordering::Relaxed), 42);
}

#[kernel_test]
fn test1() {}

#[cfg(test)]
#[test]
fn kernel_test_gen() {}
