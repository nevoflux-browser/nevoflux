//! Bounded ring buffer with DropOldest eviction policy.
//!
//! Used by the EventBus subscriber delivery layer to implement
//! backpressure when a subscriber cannot keep up with the event rate.

use std::collections::VecDeque;

/// A generic bounded ring buffer that drops the oldest item when full.
///
/// Backed by a `VecDeque<T>` with a fixed capacity. When a push would
/// exceed capacity, the oldest element is removed and returned.
///
/// # Panics
///
/// `BoundedRingBuffer::new` panics if `capacity` is 0.
pub struct BoundedRingBuffer<T> {
    buf: VecDeque<T>,
    capacity: usize,
    total_pushed: u64,
    total_dropped: u64,
}

impl<T> BoundedRingBuffer<T> {
    /// Create a new ring buffer with the given capacity.
    ///
    /// # Panics
    ///
    /// Panics if `capacity` is 0.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "BoundedRingBuffer capacity must be > 0");
        Self {
            buf: VecDeque::with_capacity(capacity),
            capacity,
            total_pushed: 0,
            total_dropped: 0,
        }
    }

    /// Push an item into the buffer.
    ///
    /// If the buffer is full, the oldest item is evicted and returned.
    /// Otherwise returns `None`.
    pub fn push(&mut self, item: T) -> Option<T> {
        self.total_pushed += 1;
        let evicted = if self.buf.len() == self.capacity {
            self.total_dropped += 1;
            self.buf.pop_front()
        } else {
            None
        };
        self.buf.push_back(item);
        evicted
    }

    /// Pop the oldest item from the buffer, or `None` if empty.
    pub fn pop(&mut self) -> Option<T> {
        self.buf.pop_front()
    }

    /// Peek at the oldest item without removing it, or `None` if empty.
    pub fn peek(&self) -> Option<&T> {
        self.buf.front()
    }

    /// Return the number of items currently in the buffer.
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// Return `true` if the buffer contains no items.
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Return `true` if the buffer is at capacity.
    pub fn is_full(&self) -> bool {
        self.buf.len() == self.capacity
    }

    /// Return the maximum capacity of the buffer.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Total number of items ever pushed into the buffer.
    pub fn total_pushed(&self) -> u64 {
        self.total_pushed
    }

    /// Total number of items evicted (dropped) due to overflow.
    pub fn total_dropped(&self) -> u64 {
        self.total_dropped
    }

    /// Drain all items from the buffer, returning them as an iterator
    /// in oldest-to-newest order.
    pub fn drain(&mut self) -> std::collections::vec_deque::Drain<'_, T> {
        self.buf.drain(..)
    }

    /// Remove all items from the buffer.
    pub fn clear(&mut self) {
        self.buf.clear();
    }

    /// Iterate over items in oldest-to-newest order.
    pub fn iter(&self) -> std::collections::vec_deque::Iter<'_, T> {
        self.buf.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_empty() {
        let rb: BoundedRingBuffer<i32> = BoundedRingBuffer::new(4);
        assert!(rb.is_empty());
        assert!(!rb.is_full());
        assert_eq!(rb.len(), 0);
        assert_eq!(rb.capacity(), 4);
        assert_eq!(rb.total_pushed(), 0);
        assert_eq!(rb.total_dropped(), 0);
    }

    #[test]
    #[should_panic(expected = "capacity must be > 0")]
    fn zero_capacity_panics() {
        let _rb: BoundedRingBuffer<i32> = BoundedRingBuffer::new(0);
    }

    #[test]
    fn push_within_capacity() {
        let mut rb = BoundedRingBuffer::new(3);
        assert_eq!(rb.push(1), None);
        assert_eq!(rb.push(2), None);
        assert_eq!(rb.push(3), None);
        assert_eq!(rb.len(), 3);
        assert!(rb.is_full());
        assert_eq!(rb.total_pushed(), 3);
        assert_eq!(rb.total_dropped(), 0);
    }

    #[test]
    fn push_drops_oldest() {
        let mut rb = BoundedRingBuffer::new(2);
        assert_eq!(rb.push(10), None);
        assert_eq!(rb.push(20), None);
        // Buffer is [10, 20], now push 30 → 10 is evicted
        assert_eq!(rb.push(30), Some(10));
        assert_eq!(rb.len(), 2);
        assert_eq!(rb.total_pushed(), 3);
        assert_eq!(rb.total_dropped(), 1);
        // Remaining: [20, 30]
        assert_eq!(rb.peek(), Some(&20));
    }

    #[test]
    fn push_multiple_overflows() {
        let mut rb = BoundedRingBuffer::new(2);
        rb.push(1);
        rb.push(2);
        assert_eq!(rb.push(3), Some(1));
        assert_eq!(rb.push(4), Some(2));
        assert_eq!(rb.push(5), Some(3));
        assert_eq!(rb.total_pushed(), 5);
        assert_eq!(rb.total_dropped(), 3);
        // Remaining: [4, 5]
        let items: Vec<_> = rb.iter().copied().collect();
        assert_eq!(items, vec![4, 5]);
    }

    #[test]
    fn pop_empty() {
        let mut rb: BoundedRingBuffer<i32> = BoundedRingBuffer::new(2);
        assert_eq!(rb.pop(), None);
    }

    #[test]
    fn peek() {
        let mut rb = BoundedRingBuffer::new(3);
        assert_eq!(rb.peek(), None);
        rb.push("a");
        rb.push("b");
        assert_eq!(rb.peek(), Some(&"a"));
        rb.pop();
        assert_eq!(rb.peek(), Some(&"b"));
    }

    #[test]
    fn drain() {
        let mut rb = BoundedRingBuffer::new(3);
        rb.push(1);
        rb.push(2);
        rb.push(3);
        let items: Vec<_> = rb.drain().collect();
        assert_eq!(items, vec![1, 2, 3]);
        assert!(rb.is_empty());
        assert_eq!(rb.len(), 0);
    }

    #[test]
    fn clear() {
        let mut rb = BoundedRingBuffer::new(3);
        rb.push(1);
        rb.push(2);
        rb.clear();
        assert!(rb.is_empty());
        assert_eq!(rb.len(), 0);
        // Counters are not reset by clear
        assert_eq!(rb.total_pushed(), 2);
    }

    #[test]
    fn iter_order() {
        let mut rb = BoundedRingBuffer::new(4);
        rb.push(10);
        rb.push(20);
        rb.push(30);
        let items: Vec<_> = rb.iter().copied().collect();
        assert_eq!(items, vec![10, 20, 30]);
    }

    #[test]
    fn capacity_one() {
        let mut rb = BoundedRingBuffer::new(1);
        assert_eq!(rb.push(1), None);
        assert!(rb.is_full());
        assert_eq!(rb.push(2), Some(1));
        assert_eq!(rb.len(), 1);
        assert_eq!(rb.peek(), Some(&2));
        assert_eq!(rb.pop(), Some(2));
        assert!(rb.is_empty());
    }
}
