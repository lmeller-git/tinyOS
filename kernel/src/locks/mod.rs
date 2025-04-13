pub mod mutex;

crate::tests! {
    #[runner]
    fn test_mutex() {
        mutex::tests::test_runner();
    }
}
