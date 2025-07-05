use testme::testme;

#[testme]
pub mod test {
    use std::time::Duration;

    async fn before_all() {
        println!("this runs before all of the tests");
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    fn after_all() {
        println!("this runs after all");
    }

    #[test]
    fn eee() {
        println!("my test!");
    }

    #[test]
    fn eee2() {
        println!("my test2!");
    }
}
