//! Lock-Free Object Pools
//!
//! Pre-allocated object pools using crossbeam's ArrayQueue for zero-allocation
//! hot paths. Objects are borrowed and returned to the pool automatically.

use crossbeam::queue::ArrayQueue;
use std::sync::Arc;

/// Lock-free object pool
///
/// Objects are pre-allocated at creation time. The pool uses
/// crossbeam's ArrayQueue for lock-free access.
///
/// # Type Parameters
/// - `T`: Type of objects in the pool (must be Default)
///
/// # Example
/// ```
/// use bog_core::perf::pools::ObjectPool;
///
/// #[derive(Default, Clone)]
/// struct MyObject {
///     data: Vec<u8>,
/// }
///
/// let pool = ObjectPool::<MyObject>::new(1024);
/// let obj = pool.acquire().expect("Pool exhausted");
/// // Use object...
/// pool.release(obj);
/// ```
pub struct ObjectPool<T: Default + Clone> {
    pool: Arc<ArrayQueue<T>>,
    capacity: usize,
}

impl<T: Default + Clone> ObjectPool<T> {
    /// Create new pool with specified capacity
    ///
    /// All objects are pre-allocated using T::default().
    /// This should be done once at startup, not in the hot path.
    pub fn new(capacity: usize) -> Self {
        let pool = Arc::new(ArrayQueue::new(capacity));

        // Pre-allocate all objects
        for _ in 0..capacity {
            pool.push(T::default()).ok();
        }

        Self { pool, capacity }
    }

    /// Acquire an object from the pool
    ///
    /// Returns None if pool is exhausted.
    /// In production HFT, pool exhaustion indicates a configuration error.
    #[inline(always)]
    pub fn acquire(&self) -> Option<T> {
        self.pool.pop()
    }

    /// Return an object to the pool
    ///
    /// If pool is full, the object is dropped (pool overflow protection).
    #[inline(always)]
    pub fn release(&self, obj: T) {
        self.pool.push(obj).ok();
    }

    /// Get pool capacity
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get current available objects in pool
    pub fn available(&self) -> usize {
        self.pool.len()
    }

    /// Check if pool is exhausted
    pub fn is_exhausted(&self) -> bool {
        self.pool.is_empty()
    }
}

impl<T: Default + Clone> Clone for ObjectPool<T> {
    fn clone(&self) -> Self {
        Self {
            pool: Arc::clone(&self.pool),
            capacity: self.capacity,
        }
    }
}

/// RAII guard for automatic object return
///
/// Ensures objects are always returned to the pool when dropped.
/// This prevents leaks and simplifies error handling.
///
/// # Example
/// ```
/// use bog_core::perf::pools::{ObjectPool, PoolGuard};
///
/// #[derive(Default, Clone)]
/// struct MyObject { data: u64 }
///
/// let pool = ObjectPool::<MyObject>::new(10);
/// {
///     let mut guard = PoolGuard::new(pool.acquire().unwrap(), pool.clone());
///     guard.data = 42;
///     // Automatically returned to pool when guard is dropped
/// }
/// ```
pub struct PoolGuard<T: Default + Clone> {
    obj: Option<T>,
    pool: ObjectPool<T>,
}

impl<T: Default + Clone> PoolGuard<T> {
    /// Create new guard
    pub fn new(obj: T, pool: ObjectPool<T>) -> Self {
        Self {
            obj: Some(obj),
            pool,
        }
    }

    /// Get reference to inner object
    pub fn get(&self) -> &T {
        self.obj.as_ref().unwrap()
    }

    /// Get mutable reference to inner object
    pub fn get_mut(&mut self) -> &mut T {
        self.obj.as_mut().unwrap()
    }
}

impl<T: Default + Clone> std::ops::Deref for PoolGuard<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.get()
    }
}

impl<T: Default + Clone> std::ops::DerefMut for PoolGuard<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.get_mut()
    }
}

impl<T: Default + Clone> Drop for PoolGuard<T> {
    fn drop(&mut self) {
        if let Some(obj) = self.obj.take() {
            self.pool.release(obj);
        }
    }
}

/// Pool statistics for monitoring
#[derive(Debug, Clone, Copy)]
pub struct PoolStats {
    pub capacity: usize,
    pub available: usize,
    pub utilization: f64,
}

impl PoolStats {
    /// Create stats from pool
    pub fn from_pool<T: Default + Clone>(pool: &ObjectPool<T>) -> Self {
        let capacity = pool.capacity();
        let available = pool.available();
        let utilization = 1.0 - (available as f64 / capacity as f64);

        Self {
            capacity,
            available,
            utilization,
        }
    }

    /// Check if pool is near exhaustion (>90% utilized)
    pub fn is_near_exhaustion(&self) -> bool {
        self.utilization > 0.9
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default, Clone, Debug, PartialEq)]
    struct TestObject {
        value: u64,
    }

    #[test]
    fn test_pool_creation() {
        let pool = ObjectPool::<TestObject>::new(10);
        assert_eq!(pool.capacity(), 10);
        assert_eq!(pool.available(), 10);
    }

    #[test]
    fn test_acquire_release() {
        let pool = ObjectPool::<TestObject>::new(5);

        let mut obj1 = pool.acquire().unwrap();
        obj1.value = 42;
        assert_eq!(pool.available(), 4);

        let obj2 = pool.acquire().unwrap();
        assert_eq!(pool.available(), 3);

        pool.release(obj1);
        assert_eq!(pool.available(), 4);

        pool.release(obj2);
        assert_eq!(pool.available(), 5);
    }

    #[test]
    fn test_pool_exhaustion() {
        let pool = ObjectPool::<TestObject>::new(2);

        let _obj1 = pool.acquire().unwrap();
        let _obj2 = pool.acquire().unwrap();

        // Pool should be exhausted
        assert!(pool.is_exhausted());
        assert_eq!(pool.acquire(), None);
    }

    #[test]
    fn test_pool_guard() {
        let pool = ObjectPool::<TestObject>::new(5);

        {
            let mut guard = PoolGuard::new(pool.acquire().unwrap(), pool.clone());
            guard.value = 99;
            assert_eq!(pool.available(), 4);
            // Guard is dropped here, object returned
        }

        // Object should be back in pool
        assert_eq!(pool.available(), 5);
    }

    #[test]
    fn test_pool_guard_deref() {
        let pool = ObjectPool::<TestObject>::new(5);
        let mut guard = PoolGuard::new(pool.acquire().unwrap(), pool.clone());

        // Test Deref
        guard.value = 42;
        assert_eq!(guard.value, 42);
    }

    #[test]
    fn test_pool_stats() {
        let pool = ObjectPool::<TestObject>::new(10);

        let _obj1 = pool.acquire();
        let _obj2 = pool.acquire();

        let stats = PoolStats::from_pool(&pool);
        assert_eq!(stats.capacity, 10);
        assert_eq!(stats.available, 8);
        assert!((stats.utilization - 0.2).abs() < 0.001);
        assert!(!stats.is_near_exhaustion());
    }

    #[test]
    fn test_pool_stats_near_exhaustion() {
        let pool = ObjectPool::<TestObject>::new(10);

        // Acquire 9 objects (90% utilization)
        let _objs: Vec<_> = (0..9).map(|_| pool.acquire().unwrap()).collect();

        let stats = PoolStats::from_pool(&pool);
        assert!(stats.is_near_exhaustion());
    }

    #[test]
    fn test_pool_clone() {
        let pool1 = ObjectPool::<TestObject>::new(5);
        let pool2 = pool1.clone();

        let _obj = pool1.acquire();
        assert_eq!(pool2.available(), 4); // Both refer to same queue
    }

    #[test]
    fn test_concurrent_access() {
        use std::thread;

        let pool = ObjectPool::<TestObject>::new(100);
        let pool_clone = pool.clone();

        let handle = thread::spawn(move || {
            for _ in 0..50 {
                if let Some(obj) = pool_clone.acquire() {
                    pool_clone.release(obj);
                }
            }
        });

        for _ in 0..50 {
            if let Some(obj) = pool.acquire() {
                pool.release(obj);
            }
        }

        handle.join().unwrap();

        // All objects should be back
        assert_eq!(pool.available(), 100);
    }
}
