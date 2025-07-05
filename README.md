# testme

This is a crate designed to be a drop-in replacement for rust unit/integration tests. Simply add the `testme` macro to the top of one of your existing test modules to get started:

```rs
use testme::testme;
#[testme]
#[cfg(test)]
mod tests {
    #[test]
    fn hello_world() {
        assert_eq!(1, 1);
    }
}
```

This crate's primary purpose is to add a way to run code before all tests, and after all tests, similarly to jest's `beforeAll` and `afterAll` functions. before all functionality is supported by adding a function within the test module called `before_all`. A full list of supported functions are:

- `before_all`: runs before all tests
- `after_all`: runs after all tests
- `before_each`: runs before each test
- `after_each`: runs after each test

```rs
use testme::testme;
#[testme]
#[cfg(test)]
mod tests {
    fn before_all() {
        println!("this runs before all the tests!");
    }
    // note the before_all and after_all functions can optionally be async:
    async fn after_all() {
        println!("this runs after all the tests");
    }
    // note tests can optionally be async
    #[test]
    async fn t1() {
        assert_eq!(1, 1);
    }

    #[test]
    #[should_panic]
    fn t2() {
        assert_eq!(1, 2);
    }
}
```

## Gotchas

this crate has a few gotchas. primarily it cannot be used to test code in targets that are not supported by [`linkme`](https://docs.rs/linkme/0.3.33/linkme/). for example: you cannot run wasm32-unknown-unknown tests.

You also cannot use `after_all` in combination with running the tests with `--test-threads=1`. If you wish to use `after_all` you must use at least 2 test threads.

## name

the name is a reference to the [`linkme`](https://docs.rs/linkme/0.3.33/linkme/) crate which is used by testme to offer `before_all` and `after_all` capabilities.
