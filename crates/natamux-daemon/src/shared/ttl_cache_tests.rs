use std::sync::atomic::{AtomicI64, AtomicU32};

use futures::channel::oneshot;
use futures::executor::block_on;

use super::*;

struct Setup {
    cache: TtlCache<u32, String>,
    clock: Arc<AtomicI64>,
    reads: Arc<AtomicU32>,
}

fn setup() -> Setup {
    let clock = Arc::new(AtomicI64::new(0));
    let now = Arc::clone(&clock);
    Setup {
        cache: TtlCache::new(1000, Arc::new(move || now.load(Ordering::SeqCst))),
        clock,
        reads: Arc::default(),
    }
}

impl Setup {
    fn read(&self) -> impl FnOnce() -> BoxFuture<'static, Result<u32, String>> + use<> {
        let reads = Arc::clone(&self.reads);
        move || async move { Ok(reads.fetch_add(1, Ordering::SeqCst) + 1) }.boxed()
    }

    fn advance(&self, ms: i64) {
        self.clock.fetch_add(ms, Ordering::SeqCst);
    }

    fn get(&self, key: &str) -> Result<u32, String> {
        block_on(self.cache.get(key, self.read()))
    }
}

/// A read that ends when the test says so.
type Read = BoxFuture<'static, Result<u32, String>>;

fn held() -> (oneshot::Sender<Result<u32, String>>, impl FnOnce() -> Read) {
    let (finish, value) = oneshot::channel();
    let read = move || async move { value.await.unwrap_or_else(|_| Err("dropped".into())) }.boxed();
    (finish, read)
}

#[test]
fn gives_the_same_answer_within_the_time_and_reads_again_after_it() {
    let s = setup();
    assert_eq!(s.get("a"), Ok(1));
    s.advance(999);
    assert_eq!(s.get("a"), Ok(1));
    s.advance(1);
    assert_eq!(s.get("a"), Ok(2));
    assert_eq!(s.reads.load(Ordering::SeqCst), 2);
}

#[test]
fn keeps_each_key_apart() {
    let s = setup();
    assert_eq!(s.get("a"), Ok(1));
    assert_eq!(s.get("b"), Ok(2));
    assert_eq!(s.get("a"), Ok(1));
}

#[test]
fn shares_a_read_while_it_runs_and_counts_the_time_from_its_end() {
    let s = setup();
    let (finish, read) = held();
    let first = s.cache.get("a", read);
    s.advance(5000);
    let second = s.cache.get("a", s.read());
    assert!(first.ptr_eq(&second));
    finish.send(Ok(7)).unwrap();
    assert_eq!(block_on(first), Ok(7));
    s.advance(999);
    assert_eq!(s.get("a"), Ok(7));
    assert_eq!(s.reads.load(Ordering::SeqCst), 0);
}

#[test]
fn does_not_keep_a_failed_read() {
    let s = setup();
    let (fail, read) = held();
    let failed = s.cache.get("a", read);
    assert!(failed.ptr_eq(&s.cache.get("a", s.read())));
    fail.send(Err("ps failed".into())).unwrap();
    assert_eq!(block_on(failed), Err("ps failed".into()));
    assert_eq!(s.get("a"), Ok(1));
}

#[test]
fn reads_again_after_it_forgets_a_key_or_every_key() {
    let s = setup();
    s.get("a").unwrap();
    s.get("b").unwrap();
    s.cache.forget(Some("a"));
    assert_eq!(s.get("a"), Ok(3));
    assert_eq!(s.get("b"), Ok(2));
    s.cache.forget(None);
    assert_eq!(s.get("a"), Ok(4));
    assert_eq!(s.get("b"), Ok(5));
}

#[test]
fn does_not_share_a_read_that_ran_when_its_key_was_forgotten() {
    let s = setup();
    let (finish, read) = held();
    let old = s.cache.get("a", read);
    s.cache.forget(Some("a"));
    let fresh = s.cache.get("a", s.read());
    finish.send(Ok(9)).unwrap();
    assert_eq!(block_on(old), Ok(9));
    assert_eq!(block_on(fresh), Ok(1));
    assert_eq!(s.get("a"), Ok(1));
}

#[test]
fn keeps_a_newer_read_when_an_older_one_that_was_forgotten_fails() {
    let s = setup();
    let (fail, read) = held();
    let old = s.cache.get("a", read);
    s.cache.forget(Some("a"));
    let fresh = s.get("a");
    fail.send(Err("late".into())).unwrap();
    assert_eq!(block_on(old), Err("late".into()));
    assert_eq!(s.get("a"), fresh);
}
