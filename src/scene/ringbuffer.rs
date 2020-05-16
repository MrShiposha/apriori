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

    pub fn first_mut(&mut self) -> Option<&mut T> {
        if self.is_empty() {
            None
        } else {
            Some(&mut self.inner[self.start_index])
        }
    }

    pub fn last(&self) -> Option<&T> {
        if self.is_empty() {
            None
        } else {
            Some(&self.inner[self.wrap_index(self.len - 1)])
        }
    }

    pub fn last_mut(&mut self) -> Option<&mut T> {
        if self.is_empty() {
            None
        } else {
            let index = self.wrap_index(self.len - 1);
            Some(&mut self.inner[index])
        }
    }

    pub fn iter(&self) -> Iter<T> {
        Iter {
            ringbuffer: self,
            index: 0,
        }
    }

    /// Add new item at the end
    /// Returns removed item at the beginning
    pub fn push_back(&mut self, el: T) -> Option<T> {
        let capacity = self.capacity();
        let index = self.wrap_raw_index(self.start_index + self.len);

        let removed = std::mem::replace(
            &mut self.inner[index],
            el
        );

        self.len += 1;
        if self.len > capacity {
            self.start_index = self.wrap_raw_index(self.start_index + 1);
            self.len = capacity;
            Some(removed)
        } else {
            None
        }
    }

    /// Add new item at the beginning
    /// Returns removed item at the end
    pub fn push_front(&mut self, el: T) -> Option<T> {
        let capacity = self.capacity();
        let (mut start_index, is_overflowed) = self.start_index.overflowing_sub(1);
        if is_overflowed {
            start_index = capacity - 1;
        }

        self.start_index = start_index;
        let removed = std::mem::replace(
            &mut self.inner[self.start_index],
            el
        );

        self.len += 1;
        if self.len > capacity {
            self.len = capacity;
            Some(removed)
        } else {
            None
        }
    }

    pub fn append<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for item in iter {
            self.push_back(item);
        }
    }

    pub fn prepend<I: IntoIterator<Item = T>>(&mut self, iter: I) {   
        for item in iter {
            self.push_front(item);
        }
    }

    pub fn truncate(&mut self, range: impl Into<TruncateRange<usize>>) -> Truncated<T> {
        let range = range.into();

        match range {
            TruncateRange::From(index) => {
                let old_len = self.len;
                if index < self.len {
                    self.len = index + 1;
                }

                Truncated::new(self, self.wrap_index(self.len), old_len - index - 1)
            }
            TruncateRange::To(mut index) => {
                if index >= self.len {
                    index = self.len - 1;
                }

                let old_start_index = self.start_index;
                self.start_index = self.wrap_index(index);
                self.len -= index;

                Truncated::new(self, old_start_index, index)
            }
        }
    }

    pub fn get(&self, index: usize) -> &T {
        unsafe {
            self.inner.get_unchecked(self.wrap_index(index))
        }
    }

    pub fn get_mut(&mut self, index: usize) -> &mut T {
        unsafe {
            let index = self.wrap_index(index);
            self.inner.get_unchecked_mut(index)
        }
    }

    fn wrap_index(&self, index: usize) -> usize {
        self.wrap_raw_index(self.start_index + index)
    }

    fn wrap_raw_index(&self, index: usize) -> usize {
        index % self.capacity()
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

pub struct Iter<'rb, T: Default + Clone> {
    ringbuffer: &'rb RingBuffer<T>,
    index: usize,
}

impl<'rb, T: Default + Clone> Iterator for Iter<'rb, T> {
    type Item = &'rb T;
    
    fn next(&mut self) -> Option<Self::Item> {
        if self.index == self.ringbuffer.len() {
            None
        } else {
            let index = self.index;
            self.index += 1;

            Some(self.ringbuffer.get(index))
        }
    }
}

pub struct Truncated<'rb, T: Default + Clone> {
    ringbuffer: &'rb mut RingBuffer<T>,
    index: usize,
    len: usize
}

impl<'rb, T: Default + Clone> Truncated<'rb, T> {
    pub fn new(
        ringbuffer: &'rb mut RingBuffer<T>, 
        index: usize, 
        len: usize
    ) -> Self {
        Self {
            ringbuffer,
            index,
            len
        }
    }

    pub fn peek_first(&mut self) -> Option<<Self as Iterator>::Item> {
        <Self as Iterator>::nth(self, 0)
    }

    pub fn peek_last(&mut self) -> Option<<Self as Iterator>::Item> {
        <Self as Iterator>::nth(self, self.len - 1)
    }

    unsafe fn get_option_mut(&mut self, index: usize) -> Option<<Self as Iterator>::Item> {
        let item = self.ringbuffer.inner.get_unchecked_mut(index);

        Some(&mut *(item as *mut _))
    }
}

impl<'rb, T: Default + Clone> Iterator for Truncated<'rb, T> {
    type Item = &'rb mut T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.len != 0 {
            unsafe {
                let old_index = self.index;
                self.index = self.ringbuffer.wrap_raw_index(self.index + 1);
                self.len -= 1;

                self.get_option_mut(old_index)
            }
        } else {
            None
        }
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        if n < self.len {
            let index = self.ringbuffer.wrap_index(self.index + n);
            unsafe {
                self.get_option_mut(index)
            }
        } else {
            None
        }
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
        let t = buffer.truncate(0..);
        assert_eq!(t.map(|i| *i).collect::<Vec<_>>(), vec![2, 3]);
        assert!(buffer.len() == 1);
        assert!(buffer[0] == 1);

        buffer.append(vec![2, 3]);
        let t = buffer.truncate(1..);
        assert_eq!(t.map(|i| *i).collect::<Vec<_>>(), vec![3]);
        assert!(buffer.len() == 2);
        assert!(buffer[0] == 1);
        assert!(buffer[1] == 2);

        buffer.push_back(3);
        let t = buffer.truncate(2..);
        assert_eq!(t.map(|i| *i).collect::<Vec<_>>(), vec![]);
        assert!(buffer.len() == 3);
        assert!(buffer[0] == 1);
        assert!(buffer[1] == 2);
        assert!(buffer[2] == 3);

        buffer.push_back(4);
        let t = buffer.truncate(1..);
        assert_eq!(t.map(|i| *i).collect::<Vec<_>>(), vec![4]);
        assert!(buffer.len() == 2);
        assert!(buffer[0] == 2);
        assert!(buffer[1] == 3);
    }

    #[test]
    fn test_truncate_left() {
        let mut buffer = RingBuffer::<u32>::new(3);
        buffer.append(vec![1, 2, 3]);

        buffer.append(vec![1, 2, 3]);
        let t = buffer.truncate(..0);
        assert!(buffer.len() == 3);
        assert!(buffer[0] == 1);
        assert!(buffer[1] == 2);
        assert!(buffer[2] == 3);

        let t = buffer.truncate(..1);
        assert!(buffer.len() == 2);
        assert!(buffer[0] == 2);
        assert!(buffer[1] == 3);

        buffer.push_front(1);
        let t = buffer.truncate(..2);
        assert!(buffer.len() == 1);
        assert!(buffer[0] == 3);

        buffer.prepend(vec![1, 2]);
        buffer.push_back(4);
        let t = buffer.truncate(..1);
        assert!(buffer.len() == 2);
        assert!(buffer[0] == 3);
        assert!(buffer[1] == 4);
    }

    #[test]
    fn test_iter() {
        let mut buffer = RingBuffer::<u32>::new(3);
        let src_vec = vec![1, 2, 3];

        buffer.append(src_vec.clone());

        let vec = buffer.iter().map(|item| *item).collect::<Vec<_>>();
        assert_eq!(vec, src_vec);
    }
}
