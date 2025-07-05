use testme::testme;

#[testme]
pub mod test {
    use std::time::Duration;
    const FILENAME: &str = "mytestfile.txt";

    /// the before*/after* functions can be async optionally
    async fn before_all() {
        tokio::time::sleep(Duration::from_secs(1)).await;
        tokio::fs::write(FILENAME, "hello").await.expect("failed to write test file");
    }

    fn after_all() {
        assert!(std::fs::exists(FILENAME).expect("failed to check existance"));
        std::fs::remove_file(FILENAME).expect("failed to remove test file");
    }

    #[test]
    fn normal_test() {
        let contents = std::fs::read_to_string(FILENAME).expect("failed to read test file");
        assert_eq!(contents, "hello");
    }

    #[test]
    async fn can_be_async_as_well() {
        let contents = tokio::fs::read_to_string(FILENAME).await.expect("failed to read test file");
        assert_eq!(contents, "hello");
    }
}


/// Note you can have multiple `#[testme]` modules
/// in one file/crate, as long as it has unique function names.
/// if this `test2` module had the exact same function names and in the exact same
/// order as the `test` module above, it wont compile
#[testme]
pub mod test2 {
    fn after_all() {
        println!("DONE!");
    }

    #[test]
    #[should_panic]
    fn some_test() {
        assert_eq!(1, 2);
    }
}
