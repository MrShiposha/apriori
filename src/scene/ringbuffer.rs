use {
    super::TruncateRange,
    std::ops::{Index, IndexMut},
};

pub struct RingBuffer<T: Default + Clone> {
    inner: Vec<T>,
    start_index: usize,
    len: usize,
}

impl<T: Default + Clone> RingBuffer<T> {
    pub fn new(size: usize) -> Self {
        assert!(size > 0);

        Self {
            inner: vec![Default::default(); size],
            start_index: 0,
            len: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn capacity(&self) -> usize {
        self.inner.len()
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn clear(&mut self) {
        self.start_index = 0;
        self.len = 0;
    }

    pub fn first(&self) -> Option<&T> {
        if self.is_empty() {
            None
        } else {
            Some(&self.inner[self.start_index])
        }
    }

    pub fn last(&self) -> Option<&T> {
        if self.is_empty() {
            None
        } else {
            Some(&self.inner[(self.start_index + self.len - 1) % self.capacity()])
        }
    }

    /// Returns true if the start_index has changed
    pub fn push_back(&mut self, el: T) -> bool {
        let capacity = self.capacity();
        self.inner[(self.start_index + self.len) % capacity] = el;

        self.len += 1;
        if self.len > capacity {
            self.start_index = (self.start_index + 1) % capacity;
            self.len = capacity;
            true
        } else {
            false
        }
    }

    pub fn push_front(&mut self, el: T) {
        let capacity = self.capacity();
        let (mut start_index, is_overflowed) = self.start_index.overflowing_sub(1);
        if is_overflowed {
            start_index = capacity - 1;
        }

        self.start_index = start_index;
        self.inner[self.start_index] = el;

        self.len += 1;
        if self.len > capacity {
            self.len = capacity;
        }
    }

    /// Returns start_index's delta
    pub fn append<I: IntoIterator<Item = T>>(&mut self, iter: I) -> i32 {
        let mut delta = 0;
        for item in iter {
            if self.push_back(item) {
                delta += 1;
            }
        }

        delta
    }

    /// Returns total objects added
    pub fn prepend<I: IntoIterator<Item = T>>(&mut self, iter: I) -> i32 {
        let mut added = 0;
        for item in iter {
            self.push_front(item);
            added += 1;
        }

        added
    }

    /// Returns start_index-delta
    pub fn truncate(&mut self, range: impl Into<TruncateRange<usize>>) -> usize {
        let range = range.into();

        match range {
            TruncateRange::From(index) => {
                if index < self.len {
                    self.len = index + 1;
                }

                0
            }
            TruncateRange::To(mut index) => {
                if index >= self.len {
                    index = self.len - 1;
                }

                self.start_index = (self.start_index + index) % self.capacity();
                self.len -= index;

                index
            }
        }
    }

    pub fn get(&self, index: usize) -> &T {
        unsafe {
            self.inner
                .get_unchecked((self.start_index + index) % self.capacity())
        }
    }

    pub fn get_mut(&mut self, index: usize) -> &mut T {
        let capacity = self.capacity();
        unsafe {
            self.inner
                .get_unchecked_mut((self.start_index + index) % capacity)
        }
    }
}

impl<T> Index<usize> for RingBuffer<T>
where
    T: Default + Clone,
{
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index)
    }
}

impl<T> IndexMut<usize> for RingBuffer<T>
where
    T: Default + Clone,
{
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.get_mut(index)
    }
}

#[cfg(test)]
mod tests {
    use crate::scene::ringbuffer::RingBuffer;

    #[test]
    fn test_push_back() {
        let mut buffer = RingBuffer::<u32>::new(3);
        assert!(buffer.is_empty());
        assert!(buffer.len() == 0);

        buffer.push_back(1);
        assert!(!buffer.is_empty());
        assert!(buffer.len() == 1);
        assert_eq!(buffer[0], 1);

        buffer.push_back(2);
        assert!(!buffer.is_empty());
        assert!(buffer.len() == 2);
        assert_eq!(buffer[0], 1);
        assert_eq!(buffer[1], 2);

        buffer.push_back(3);
        assert!(!buffer.is_empty());
        assert!(buffer.len() == 3);
        assert_eq!(buffer[0], 1);
        assert_eq!(buffer[1], 2);
        assert_eq!(buffer[2], 3);

        buffer.push_back(4);
        assert!(!buffer.is_empty());
        assert!(buffer.len() == 3);
        assert_eq!(buffer[0], 2);
        assert_eq!(buffer[1], 3);
        assert_eq!(buffer[2], 4);

        buffer.push_back(5);
        assert!(!buffer.is_empty());
        assert!(buffer.len() == 3);
        assert_eq!(buffer[0], 3);
        assert_eq!(buffer[1], 4);
        assert_eq!(buffer[2], 5);

        buffer.push_back(6);
        assert!(!buffer.is_empty());
        assert!(buffer.len() == 3);
        assert_eq!(buffer[0], 4);
        assert_eq!(buffer[1], 5);
        assert_eq!(buffer[2], 6);
    }

    #[test]
    fn test_push_front() {
        let mut buffer = RingBuffer::<u32>::new(3);
        assert!(buffer.is_empty());
        assert!(buffer.len() == 0);

        buffer.push_front(1);
        assert!(!buffer.is_empty());
        assert!(buffer.len() == 1);
        assert_eq!(buffer[0], 1);

        buffer.push_front(2);
        assert!(!buffer.is_empty());
        assert!(buffer.len() == 2);
        assert_eq!(buffer[0], 2);
        assert_eq!(buffer[1], 1);

        buffer.push_front(3);
        assert!(!buffer.is_empty());
        assert!(buffer.len() == 3);
        assert_eq!(buffer[0], 3);
        assert_eq!(buffer[1], 2);
        assert_eq!(buffer[2], 1);

        buffer.push_front(4);
        assert!(!buffer.is_empty());
        assert!(buffer.len() == 3);
        assert_eq!(buffer[0], 4);
        assert_eq!(buffer[1], 3);
        assert_eq!(buffer[2], 2);

        buffer.push_front(5);
        assert!(!buffer.is_empty());
        assert!(buffer.len() == 3);
        assert_eq!(buffer[0], 5);
        assert_eq!(buffer[1], 4);
        assert_eq!(buffer[2], 3);

        buffer.push_front(6);
        assert!(!buffer.is_empty());
        assert!(buffer.len() == 3);
        assert_eq!(buffer[0], 6);
        assert_eq!(buffer[1], 5);
        assert_eq!(buffer[2], 4);
    }

    #[test]
    fn test_append() {
        let mut buffer = RingBuffer::<u32>::new(3);
        assert!(buffer.is_empty());
        assert!(buffer.len() == 0);

        buffer.append(vec![1, 2, 3]);

        assert!(!buffer.is_empty());
        assert!(buffer.len() == 3);
        assert_eq!(buffer[0], 1);
        assert_eq!(buffer[1], 2);
        assert_eq!(buffer[2], 3);
    }

    #[test]
    fn test_prepend() {
        let mut buffer = RingBuffer::<u32>::new(3);
        assert!(buffer.is_empty());
        assert!(buffer.len() == 0);

        buffer.prepend(vec![1, 2, 3]);

        assert!(!buffer.is_empty());
        assert!(buffer.len() == 3);
        assert_eq!(buffer[0], 3);
        assert_eq!(buffer[1], 2);
        assert_eq!(buffer[2], 1);
    }

    #[test]
    fn test_front_if_full() {
        let mut buffer = RingBuffer::<u32>::new(3);
        assert!(buffer.is_empty());
        assert!(buffer.len() == 0);

        buffer.append(vec![1, 2, 3]);
        assert!(!buffer.is_empty());
        assert!(buffer.len() == 3);
        assert_eq!(buffer[0], 1);
        assert_eq!(buffer[1], 2);
        assert_eq!(buffer[2], 3);

        buffer.push_front(4);
        assert!(!buffer.is_empty());
        assert!(buffer.len() == 3);
        assert_eq!(buffer[0], 4);
        assert_eq!(buffer[1], 1);
        assert_eq!(buffer[2], 2);

        buffer.push_front(5);
        assert!(!buffer.is_empty());
        assert!(buffer.len() == 3);
        assert_eq!(buffer[0], 5);
        assert_eq!(buffer[1], 4);
        assert_eq!(buffer[2], 1);

        buffer.push_front(6);
        assert!(!buffer.is_empty());
        assert!(buffer.len() == 3);
        assert_eq!(buffer[0], 6);
        assert_eq!(buffer[1], 5);
        assert_eq!(buffer[2], 4);
    }

    #[test]
    fn test_back_if_full() {
        let mut buffer = RingBuffer::<u32>::new(3);
        assert!(buffer.is_empty());
        assert!(buffer.len() == 0);

        buffer.prepend(vec![1, 2, 3]);
        assert!(!buffer.is_empty());
        assert!(buffer.len() == 3);
        assert_eq!(buffer[0], 3);
        assert_eq!(buffer[1], 2);
        assert_eq!(buffer[2], 1);

        buffer.push_back(4);
        assert!(!buffer.is_empty());
        assert!(buffer.len() == 3);
        assert_eq!(buffer[0], 2);
        assert_eq!(buffer[1], 1);
        assert_eq!(buffer[2], 4);

        buffer.push_back(5);
        assert!(!buffer.is_empty());
        assert!(buffer.len() == 3);
        assert_eq!(buffer[0], 1);
        assert_eq!(buffer[1], 4);
        assert_eq!(buffer[2], 5);

        buffer.push_back(6);
        assert!(!buffer.is_empty());
        assert!(buffer.len() == 3);
        assert_eq!(buffer[0], 4);
        assert_eq!(buffer[1], 5);
        assert_eq!(buffer[2], 6);
    }

    #[test]
    fn test_truncate_right() {
        let mut buffer = RingBuffer::<u32>::new(3);
        buffer.append(vec![1, 2, 3]);

        buffer.append(vec![1, 2, 3]);
        assert_eq!(buffer.truncate(0..), 0);
        assert!(buffer.len() == 1);
        assert!(buffer[0] == 1);

        buffer.append(vec![2, 3]);
        assert_eq!(buffer.truncate(1..), 0);
        assert!(buffer.len() == 2);
        assert!(buffer[0] == 1);
        assert!(buffer[1] == 2);

        buffer.push_back(3);
        assert_eq!(buffer.truncate(2..), 0);
        assert!(buffer.len() == 3);
        assert!(buffer[0] == 1);
        assert!(buffer[1] == 2);
        assert!(buffer[2] == 3);

        buffer.push_back(4);
        assert_eq!(buffer.truncate(1..), 0);
        assert!(buffer.len() == 2);
        assert!(buffer[0] == 2);
        assert!(buffer[1] == 3);
    }

    #[test]
    fn test_truncate_left() {
        let mut buffer = RingBuffer::<u32>::new(3);
        buffer.append(vec![1, 2, 3]);

        buffer.append(vec![1, 2, 3]);
        assert_eq!(buffer.truncate(..0), 0);
        assert!(buffer.len() == 3);
        assert!(buffer[0] == 1);
        assert!(buffer[1] == 2);
        assert!(buffer[2] == 3);

        assert_eq!(buffer.truncate(..1), 1);
        assert!(buffer.len() == 2);
        assert!(buffer[0] == 2);
        assert!(buffer[1] == 3);

        buffer.push_front(1);
        assert_eq!(buffer.truncate(..2), 2);
        assert!(buffer.len() == 1);
        assert!(buffer[0] == 3);

        buffer.prepend(vec![1, 2]);
        buffer.push_back(4);
        assert_eq!(buffer.truncate(..1), 1);
        assert!(buffer.len() == 2);
        assert!(buffer[0] == 3);
        assert!(buffer[1] == 4);
    }
}
