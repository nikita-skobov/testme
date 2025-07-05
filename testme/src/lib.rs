use std::{sync::{Mutex, OnceLock}, time::Duration};

pub use linkme::distributed_slice;
use linkme::DistributedSlice;
pub use testme_derive::testme;
use tokio::{sync::oneshot::{Receiver, Sender}, task::JoinError};

/// reads the command line arg that the test was invoked with
/// to see if we should add a loop delay. for most cases the answer should be yes.
/// the only time we dont want to loop/delay is if we're running a single test with --exact
/// or if we are running with --test-threads=1
pub fn should_loop(
    after_all: DistributedSlice<[fn() -> std::pin::Pin<Box<dyn Future<Output = ()>>>]>
) -> bool {
    let args: Vec<String> = std::env::args().collect();
    // if running just 1 test, or running tests sequentially, theres no need
    // to loop and wait for more tests to submit their handles. we can just run the runtime once
    // and exit. if running sequentially, the next test will create a new runtime.
    let mut should_loop = true;
    if args.contains(&"--exact".to_string()) {
        should_loop = false;
    } else if args.contains(&"--test-threads=1".to_string()) {
        if !after_all.is_empty() {
            panic!("cannot run tests with --test-threads=1 and also use AFTER_ALL");
        }
        should_loop = false;
    } else {
        let test_threads_index = args.iter().position(|x| x == "--test-threads");
        if let Some(i) = test_threads_index {
            if let Some(x) = args.get(i + 1) {
                if x == "1" {
                    if !after_all.is_empty() {
                        panic!("cannot run tests with --test-threads=1 and also use AFTER_ALL");
                    }
                    should_loop = false;
                }
            }
        }
    }
    should_loop
}

pub struct Test {
    pub blocking_fn: Option<Box<dyn FnOnce() + Send + 'static>>,
    pub fut: std::pin::Pin<Box<dyn Future<Output = ()> + Send + Sync>>,
    pub callback: Sender<Result<(), JoinError>>,
}

pub fn submit_test(
    run_all_lock: &'static OnceLock<Mutex<()>>,
    test_handles: &'static OnceLock<Mutex<Vec<Test>>>,
    before_each: DistributedSlice<[fn() -> std::pin::Pin<Box<dyn Future<Output=()> + Send>>]>,
    before_all: DistributedSlice<[fn() -> std::pin::Pin<Box<dyn Future<Output = ()>>>]>,
    after_each: DistributedSlice<[fn() -> std::pin::Pin<Box<dyn Future<Output=()> + Send>>]>,
    after_all: DistributedSlice<[fn() -> std::pin::Pin<Box<dyn Future<Output = ()>>>]>,
    rx: Receiver<Result<(), JoinError>>,
    t: Test,
) {
    let mutex: &'static Mutex<Vec<Test>> = test_handles.get_or_init(|| Mutex::new(vec![]));
    let mut handles = mutex.lock().expect("failed to get mutex lock");
    handles.push(t);
    drop(handles);

    run_all_tests(
        run_all_lock,
        mutex,
        before_each,
        before_all,
        after_each,
        after_all
    );
    if let Err(e) = rx.blocking_recv().expect("tests ended before receiving a result") {
        if let Ok(reason) = e.try_into_panic() {
            std::panic::resume_unwind(reason);
        } else {
            panic!("test case was cancellled?");
        }
    }
}

pub fn run_all_tests(
    run_all_lock: &'static OnceLock<Mutex<()>>,
    test_handles: &'static Mutex<Vec<Test>>,
    before_each: DistributedSlice<[fn() -> std::pin::Pin<Box<dyn Future<Output=()> + Send>>]>,
    before_all: DistributedSlice<[fn() -> std::pin::Pin<Box<dyn Future<Output = ()>>>]>,
    after_each: DistributedSlice<[fn() -> std::pin::Pin<Box<dyn Future<Output=()> + Send>>]>,
    after_all: DistributedSlice<[fn() -> std::pin::Pin<Box<dyn Future<Output = ()>>>]>,
) {
    let run_all_mutex = run_all_lock.get_or_init(|| Mutex::new(()));
    if let Ok(l) = run_all_mutex.try_lock() {
        let should_loop = should_loop(after_all);
        // if we got the lock, then we are the test that will drive the others.
        // we will have one runtime, and all the other tests will be ran in this runtime
        let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("failed to build tokio runtime");
        rt.block_on(async move {
            let (spawner_tx, mut spawner_rx) = tokio::sync::mpsc::unbounded_channel();
            // run the before all first:
            for before_all in before_all.iter() {
                let fut = before_all();
                fut.await;
            }
            // a task will periodically check the test handles and spawn tasks
            tokio::task::spawn(async move {
                let mut wait_ms = 0;
                let max_consecutive_loops_without_tests = 2;
                let mut consecutive_loops_without_tests_spawned = 0;
                loop {
                    tokio::time::sleep(Duration::from_millis(wait_ms)).await;
                    if consecutive_loops_without_tests_spawned >= max_consecutive_loops_without_tests {
                        break;
                    }
                    let mut test_handles = test_handles.lock().expect("failed to get lock on test handles");
                    let drained: Vec<Test> = test_handles.drain(..).collect();
                    drop(test_handles);
                    if drained.is_empty() {
                        consecutive_loops_without_tests_spawned += 1;
                    } else {
                        consecutive_loops_without_tests_spawned = 0;
                    }
                    for test in drained {
                        let fut = test.fut;
                        let callback = test.callback;
                        let spawner_tx_clone = spawner_tx.clone();
                        tokio::task::spawn(async move {
                            for before_each in before_each.iter() {
                                let before_fut = before_each();
                                before_fut.await;
                            }
                            let res = if let Some(blocking) = test.blocking_fn {
                                tokio::task::spawn_blocking(blocking)
                            } else {
                                tokio::task::spawn(async move {
                                    fut.await
                                })
                            };
                            let res = res.await;
                            spawner_tx_clone.send(()).expect("spawner rx already closed?");
                            for after_each in after_each.iter() {
                                let after_fut = after_each();
                                after_fut.await;
                            }
                            callback.send(res).expect("test stopped polling its result callback");
                        });
                    }
                    if !should_loop {
                        break;
                    }
                    wait_ms = 500;
                }
            });
            // ensures we wait until all the spawned test tasks finish before we
            // exit the runtime
            while let Some(_) = spawner_rx.recv().await {}

            // after all are done we can run after_all
            for after_all in after_all.iter() {
                let fut = after_all();
                fut.await;
            }
            drop(l);
        });
    }
}
