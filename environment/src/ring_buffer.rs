//! A fixed-capacity circular buffer, used for the two ordered zones whose
//! cards come and go at both ends: the deck (drawn from the top, pitched cards
//! put on the bottom) and the stack (layers added and resolved at the top).
//!
//! `head` and `tail` are free-running `u8` counters: `head` is the position of
//! the front element and `tail` one past the back, both reduced modulo the
//! capacity `N` only when indexing. Because they never wrap at `N`, an empty
//! buffer (`head == tail`) and a full one (`tail - head == N`) are told apart
//! without giving up a slot. `N` must be a power of two no larger than 128, so
//! that the `u8` counters wrap at a multiple of `N`.

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RingBuffer<T: Copy, const N: usize> {
    items: [T; N],
    head: u8,
    tail: u8,
}

// A general-purpose container: not every accessor has an engine caller yet.
#[allow(dead_code)]
impl<T: Copy, const N: usize> RingBuffer<T, N> {
    const MASK: u8 = {
        assert!(N.is_power_of_two() && N <= 128, "RingBuffer capacity must be a power of two <= 128");
        (N - 1) as u8
    };

    /// An empty buffer. `fill` is only the value unused slots hold; it is
    /// never read back.
    pub const fn new(fill: T) -> Self {
        let _ = Self::MASK;
        RingBuffer { items: [fill; N], head: 0, tail: 0 }
    }

    #[inline]
    fn slot(counter: u8) -> usize {
        (counter & Self::MASK) as usize
    }

    /// The counter of the front element: where the buffer currently starts.
    #[inline]
    pub const fn head(&self) -> u8 {
        self.head
    }

    /// One past the counter of the back element.
    #[inline]
    pub const fn tail(&self) -> u8 {
        self.tail
    }

    #[inline]
    pub const fn len(&self) -> usize {
        self.tail.wrapping_sub(self.head) as usize
    }

    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.head == self.tail
    }

    #[inline]
    pub const fn capacity(&self) -> usize {
        N
    }

    /// The `i`th element counting from the front, if there is one.
    #[inline]
    pub fn get(&self, i: usize) -> Option<T> {
        (i < self.len()).then(|| self.items[Self::slot(self.head.wrapping_add(i as u8))])
    }

    #[inline]
    pub fn front(&self) -> Option<T> {
        self.get(0)
    }

    #[inline]
    pub fn back(&self) -> Option<T> {
        self.len().checked_sub(1).and_then(|i| self.get(i))
    }

    /// Mutable access to the `i`th element counting from the front.
    #[inline]
    pub fn get_mut(&mut self, i: usize) -> Option<&mut T> {
        if i < self.len() {
            Some(&mut self.items[Self::slot(self.head.wrapping_add(i as u8))])
        } else {
            None
        }
    }

    /// Add `item` after the back. Returns `Err(item)` when the buffer is full.
    #[inline]
    pub fn try_push_back(&mut self, item: T) -> Result<(), T> {
        if self.len() == N {
            return Err(item);
        }
        self.items[Self::slot(self.tail)] = item;
        self.tail = self.tail.wrapping_add(1);
        Ok(())
    }

    /// Add `item` before the front. Returns `Err(item)` when the buffer is full.
    #[inline]
    pub fn try_push_front(&mut self, item: T) -> Result<(), T> {
        if self.len() == N {
            return Err(item);
        }
        self.head = self.head.wrapping_sub(1);
        self.items[Self::slot(self.head)] = item;
        Ok(())
    }

    /// `try_push_back`, panicking when full.
    #[inline]
    pub fn push_back(&mut self, item: T) {
        if self.try_push_back(item).is_err() {
            panic!("RingBuffer of capacity {} is full", N);
        }
    }

    /// `try_push_front`, panicking when full.
    #[inline]
    pub fn push_front(&mut self, item: T) {
        if self.try_push_front(item).is_err() {
            panic!("RingBuffer of capacity {} is full", N);
        }
    }

    #[inline]
    pub fn pop_front(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }
        let item = self.items[Self::slot(self.head)];
        self.head = self.head.wrapping_add(1);
        Some(item)
    }

    #[inline]
    pub fn pop_back(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }
        self.tail = self.tail.wrapping_sub(1);
        Some(self.items[Self::slot(self.tail)])
    }

    /// Remove and return the `i`th element counting from the front, closing
    /// the gap by shifting the elements behind it forward. `None` when `i` is
    /// past the back.
    pub fn remove(&mut self, i: usize) -> Option<T> {
        let item = self.get(i)?;
        for j in i..self.len() - 1 {
            let next = self.items[Self::slot(self.head.wrapping_add(j as u8 + 1))];
            self.items[Self::slot(self.head.wrapping_add(j as u8))] = next;
        }
        self.tail = self.tail.wrapping_sub(1);
        Some(item)
    }

    pub fn clear(&mut self) {
        self.head = 0;
        self.tail = 0;
    }

    /// Every element, front to back.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = T> + ExactSizeIterator + '_ {
        (0..self.len()).map(move |i| self.items[Self::slot(self.head.wrapping_add(i as u8))])
    }

    /// Rotate the elements to the start of the backing array and return them
    /// as one slice, front first, so they can be rearranged in place (e.g.
    /// shuffled).
    pub fn make_contiguous(&mut self) -> &mut [T] {
        let len = self.len();
        let start = Self::slot(self.head);
        self.items.rotate_left(start);
        self.head = 0;
        self.tail = len as u8;
        &mut self.items[..len]
    }
}

impl<T: Copy + std::fmt::Debug, const N: usize> std::fmt::Debug for RingBuffer<T, N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contents<const N: usize>(b: &RingBuffer<u8, N>) -> Vec<u8> {
        b.iter().collect()
    }

    #[test]
    fn push_and_pop_at_both_ends() {
        let mut b: RingBuffer<u8, 8> = RingBuffer::new(0);
        assert!(b.is_empty());
        b.push_back(1);
        b.push_back(2);
        b.push_front(0);
        assert_eq!(contents(&b), vec![0, 1, 2]);
        assert_eq!(b.front(), Some(0));
        assert_eq!(b.back(), Some(2));
        assert_eq!(b.pop_back(), Some(2));
        assert_eq!(b.pop_front(), Some(0));
        assert_eq!(b.pop_front(), Some(1));
        assert_eq!(b.pop_front(), None);
        assert_eq!(b.pop_back(), None);
    }

    #[test]
    fn holds_its_full_capacity_across_the_wrap() {
        let mut b: RingBuffer<u8, 4> = RingBuffer::new(0);
        // Walk the head well past several wraps of the u8 counters.
        for round in 0..200u8 {
            for i in 0..4 {
                b.push_back(round.wrapping_add(i));
            }
            assert_eq!(b.len(), 4);
            assert!(b.try_push_back(9).is_err());
            assert!(b.try_push_front(9).is_err());
            for i in 0..4 {
                assert_eq!(b.pop_front(), Some(round.wrapping_add(i)));
            }
            assert!(b.is_empty());
        }
    }

    #[test]
    fn remove_closes_the_gap() {
        let mut b: RingBuffer<u8, 8> = RingBuffer::new(0);
        // Start near the end of the backing array so the contents wrap.
        for _ in 0..6 {
            b.push_back(0);
            b.pop_front();
        }
        for i in 1..=5 {
            b.push_back(i);
        }
        assert_eq!(b.remove(1), Some(2));
        assert_eq!(contents(&b), vec![1, 3, 4, 5]);
        assert_eq!(b.remove(3), Some(5));
        assert_eq!(b.remove(0), Some(1));
        assert_eq!(contents(&b), vec![3, 4]);
        assert_eq!(b.remove(2), None);
    }

    #[test]
    fn make_contiguous_keeps_the_order() {
        let mut b: RingBuffer<u8, 8> = RingBuffer::new(0);
        for _ in 0..5 {
            b.push_back(0);
            b.pop_front();
        }
        for i in 1..=6 {
            b.push_back(i);
        }
        assert_eq!(b.make_contiguous(), &[1, 2, 3, 4, 5, 6]);
        assert_eq!(contents(&b), vec![1, 2, 3, 4, 5, 6]);
        b.push_front(0);
        assert_eq!(contents(&b), vec![0, 1, 2, 3, 4, 5, 6]);
    }
}
