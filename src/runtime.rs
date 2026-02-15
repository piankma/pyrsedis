//! Global tokio runtime management.
//!
//! Provides a shared tokio multi-threaded runtime that lives for the lifetime
//! of the Python process. All async I/O (Redis connections, sentinel monitoring,
//! etc.) runs on this runtime's thread pool.

use std::sync::OnceLock;
use tokio::runtime::Runtime;

/// Global tokio runtime, initialized once on first use.
static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// Get (or initialize) the global tokio runtime.
///
/// The runtime is multi-threaded with the default number of worker threads
/// (typically equal to the number of CPU cores). Override with the
/// `PYRSEDIS_RUNTIME_THREADS` environment variable.
pub fn get_runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        let mut builder = tokio::runtime::Builder::new_multi_thread();
        builder.enable_all();

        // Allow overriding thread count
        if let Ok(threads) = std::env::var("PYRSEDIS_RUNTIME_THREADS") {
            if let Ok(n) = threads.parse::<usize>() {
                if n > 0 {
                    builder.worker_threads(n);
                }
            }
        }

        match builder.thread_name("pyrsedis-rt").build() {
            Ok(rt) => rt,
            Err(e) => {
                // Cannot return an error from OnceLock::get_or_init, so we
                // must panic here. This is acceptable because runtime creation
                // failure (e.g. ulimit too low) is unrecoverable. PyO3 will
                // catch the panic at the FFI boundary and convert it to a
                // Python RuntimeError.
                panic!("pyrsedis: failed to create tokio runtime: {e}");
            }
        }
    })
}

/// Block on a future using the global runtime.
///
/// This is the primary bridge between synchronous PyO3 code and async Rust.
/// Note: This must NOT be called from within an async context (will panic).
pub fn block_on<F: std::future::Future>(future: F) -> F::Output {
    get_runtime().block_on(future)
}

/// Spawn a future on the global runtime.
///
/// Returns a `JoinHandle` that can be awaited.
pub fn spawn<F>(future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    get_runtime().spawn(future)
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_initializes() {
        let rt = get_runtime();
        // Verify we can block on a trivial future
        let result = rt.block_on(async { 42 });
        assert_eq!(result, 42);
    }

    #[test]
    fn runtime_is_same_instance() {
        let rt1 = get_runtime();
        let rt2 = get_runtime();
        // Both should be the same pointer
        assert!(std::ptr::eq(rt1, rt2));
    }

    #[test]
    fn block_on_works() {
        let result = block_on(async { "hello" });
        assert_eq!(result, "hello");
    }

    #[test]
    fn spawn_works() {
        let handle = spawn(async { 123 });
        let result = block_on(handle).unwrap();
        assert_eq!(result, 123);
    }

    #[test]
    fn spawn_multiple() {
        let handles: Vec<_> = (0..10).map(|i| spawn(async move { i * 2 })).collect();
        let results: Vec<_> = block_on(async {
            let mut results = Vec::new();
            for h in handles {
                results.push(h.await.unwrap());
            }
            results
        });
        assert_eq!(results, vec![0, 2, 4, 6, 8, 10, 12, 14, 16, 18]);
    }

    #[test]
    fn runtime_supports_timer() {
        block_on(async {
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        });
        // If we get here, timer worked
    }

    #[test]
    fn runtime_supports_channels() {
        block_on(async {
            let (tx, rx) = tokio::sync::oneshot::channel();
            tokio::spawn(async move {
                tx.send(42).unwrap();
            });
            let val = rx.await.unwrap();
            assert_eq!(val, 42);
        });
    }
}
