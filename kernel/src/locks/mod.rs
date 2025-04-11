pub mod mutex;

crate::tests! {
    #[test_case]
    fn test_mutex() {
        mutex::tests::test_runner.run();
    }
}
