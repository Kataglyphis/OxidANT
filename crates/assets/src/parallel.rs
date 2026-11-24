//! A parallel map for the loaders' per-texture work, and a byte budget its items share; wasm32 maps in order.

use std::sync::{Condvar, Mutex, PoisonError};

/// `f` over `items` on up to eight cores, results in `items`' order; a panic in `f` resumes on the calling thread.
pub(crate) fn map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    #[cfg(target_arch = "wasm32")]
    {
        items.iter().map(f).collect()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let workers = std::thread::available_parallelism()
            .map_or(1, usize::from)
            .min(8)
            .min(items.len());
        if workers <= 1 {
            return items.iter().map(f).collect();
        }
        let next = AtomicUsize::new(0);
        let work = || {
            let mut done = Vec::new();
            loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(item) = items.get(index) else {
                    return done;
                };
                done.push((index, f(item)));
            }
        };
        let mut results: Vec<Option<R>> =
            std::iter::repeat_with(|| None).take(items.len()).collect();
        std::thread::scope(|scope| {
            // A worker that cannot start leaves its share to the others and to this thread.
            let helpers: Vec<_> = (1..workers)
                .filter_map(|_| std::thread::Builder::new().spawn_scoped(scope, work).ok())
                .collect();
            let mut finished = vec![work()];
            for helper in helpers {
                match helper.join() {
                    Ok(done) => finished.push(done),
                    Err(panic) => std::panic::resume_unwind(panic),
                }
            }
            for (index, result) in finished.into_iter().flatten() {
                results[index] = Some(result);
            }
        });
        results.into_iter().flatten().collect()
    }
}

/// Bytes the items of one map may hold at once: eight 16384x16384 decodes would not fit an 8 GB machine.
pub(crate) struct Budget {
    limit: u64,
    used: Mutex<u64>,
    freed: Condvar,
}

/// Returns its bytes to the budget when dropped.
pub(crate) struct Lease<'a> {
    budget: &'a Budget,
    amount: u64,
}

impl Budget {
    pub(crate) fn new(limit: u64) -> Budget {
        Budget {
            limit,
            used: Mutex::new(0),
            freed: Condvar::new(),
        }
    }

    /// Waits until `amount` fits beside what is held; an item over the whole budget runs once nothing else does.
    pub(crate) fn lease(&self, amount: u64) -> Lease<'_> {
        let mut used = self.used.lock().unwrap_or_else(PoisonError::into_inner);
        while *used > 0 && *used + amount > self.limit {
            used = self
                .freed
                .wait(used)
                .unwrap_or_else(PoisonError::into_inner);
        }
        *used += amount;
        Lease {
            budget: self,
            amount,
        }
    }
}

impl Drop for Lease<'_> {
    fn drop(&mut self) {
        let mut used = self
            .budget
            .used
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *used -= self.amount;
        self.budget.freed.notify_all();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn results_keep_the_items_order() {
        let items: Vec<u64> = (0..100).collect();
        let squares = super::map(&items, |&i| {
            // Uneven work, so the workers finish out of order.
            std::thread::sleep(std::time::Duration::from_micros((100 - i) * 10));
            i * i
        });
        assert_eq!(squares, items.iter().map(|i| i * i).collect::<Vec<_>>());
        assert!(super::map(&[] as &[u8], |&b| b).is_empty());
    }

    #[test]
    fn the_budget_holds_what_is_leased_at_once() {
        use std::sync::atomic::{AtomicU64, Ordering};
        let budget = super::Budget::new(100);
        let held = AtomicU64::new(0);
        let peak = AtomicU64::new(0);
        let sizes: Vec<u64> = vec![60, 30, 50, 20, 70, 10, 40, 90, 200];
        super::map(&sizes, |&size| {
            let _lease = budget.lease(size);
            let now = held.fetch_add(size, Ordering::SeqCst) + size;
            peak.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(5));
            held.fetch_sub(size, Ordering::SeqCst);
        });
        // 200 is over the whole budget and runs alone.
        assert!(
            peak.load(Ordering::SeqCst) <= 200,
            "{}",
            peak.load(Ordering::SeqCst)
        );
        let small_peak = {
            let budget = super::Budget::new(100);
            peak.store(0, Ordering::SeqCst);
            super::map(&sizes[..8], |&size| {
                let _lease = budget.lease(size);
                let now = held.fetch_add(size, Ordering::SeqCst) + size;
                peak.fetch_max(now, Ordering::SeqCst);
                std::thread::sleep(std::time::Duration::from_millis(5));
                held.fetch_sub(size, Ordering::SeqCst);
            });
            peak.load(Ordering::SeqCst)
        };
        assert!(small_peak <= 100, "{small_peak} bytes held at once");
    }

    #[test]
    fn a_panic_in_a_worker_reaches_the_caller() {
        let items: Vec<u32> = (0..32).collect();
        let caught = std::panic::catch_unwind(|| {
            super::map(&items, |&i| {
                assert!(i != 17, "item 17");
                i
            })
        });
        assert!(caught.is_err());
    }
}
