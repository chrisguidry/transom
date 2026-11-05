//! A Redis for tests that are not written in Rust, such as the Android
//! app's. It prints the Redis URL on the first line of standard output
//! once Redis answers, and keeps that Redis until its standard input
//! closes, which happens when the process that started it exits, however
//! it exits. With `TRANSOM_TEST_REDIS_URL` set, it prints that URL and
//! starts nothing.
//!
//! ```sh
//! cargo run -p test-redis    # Ctrl-D or Ctrl-C removes the container
//! ```

fn main() {
    println!("{}", test_redis::url());
    std::io::copy(&mut std::io::stdin(), &mut std::io::sink()).unwrap();
}
