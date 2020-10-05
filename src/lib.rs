#![forbid(unsafe_code)]

use std::marker::PhantomData;

#[derive(Clone)]
pub struct FlatVec<T> {
    data: Vec<u8>,
    ends: Vec<usize>,
    marker: PhantomData<T>,
}

impl<T> std::fmt::Debug for FlatVec<T> {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        fmt.debug_struct("FlatVec")
            .field("data", &self.data)
            .field("ends", &self.ends)
            .finish()
    }
}

impl<T> Default for FlatVec<T> {
    #[inline]
    fn default() -> Self {
        Self {
            data: Vec::new(),
            ends: Vec::new(),
            marker: PhantomData::default(),
        }
    }
}

impl<T> FlatVec<T> {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.ends.len()
    }

    #[inline]
    pub fn data_len(&self) -> usize {
        self.data.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.ends.len() == 0
    }

    #[inline]
    pub fn pop<Erected>(&mut self) -> Option<Erected>
    where
        Erected: ErectFrom<T>,
    {
        let end = *self.ends.last()?;
        let start = *self.ends.iter().rev().nth(1).unwrap_or(&0);
        let output = Erected::erect_from(&self.data[start..end]);
        self.data.truncate(start);
        self.ends.pop();
        Some(output)
    }

    #[inline]
    pub fn remove(&mut self, index: usize) {
        let end = self.ends[index];
        let start = if index == 0 { 0 } else { self.ends[index - 1] };
        self.data.drain(start..end);
        self.ends.remove(index);
        let len = end - start;
        self.ends.iter_mut().skip(index).for_each(|end| *end -= len);
    }

    #[inline]
    pub fn push<Source>(&mut self, input: &Source)
    where
        Source: FlattenInto<T> + ?Sized,
    {
        input.flatten_into(Storage(&mut self.data));
        self.ends.push(self.data.len());
    }

    #[inline]
    pub fn into_iter<Erected: ErectFrom<T>>(self) -> FlatVecIntoIter<Erected, T> {
        FlatVecIntoIter {
            inner: self,
            cursor: 0,
            marker: std::marker::PhantomData::default(),
        }
    }
}

pub struct Storage<'a>(&'a mut Vec<u8>);

impl Storage<'_> {
    #[inline]
    pub fn reserve(&mut self, len: usize) {
        self.0.reserve(len);
    }

    #[inline]
    pub fn extend<Iter, Bor>(&mut self, iter: Iter)
    where
        Iter: IntoIterator<Item = Bor>,
        Bor: std::borrow::Borrow<u8>,
    {
        self.0.extend(iter.into_iter().map(|b| *b.borrow()));
    }
}

pub trait FlattenInto<Flattened> {
    fn flatten_into(&self, storage: Storage);
}

pub trait ErectFrom<Flattened> {
    fn erect_from(data: &[u8]) -> Self;
}

pub struct FlatVecIntoIter<Erected, T> {
    inner: FlatVec<T>,
    cursor: usize,
    marker: std::marker::PhantomData<Erected>,
}

impl<Erected, T> Iterator for FlatVecIntoIter<Erected, T>
where
    Erected: ErectFrom<T>,
{
    type Item = Erected;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let end = *self.inner.ends.get(self.cursor)?;
        let start = if self.cursor == 0 {
            0
        } else {
            self.inner.ends[self.cursor - 1]
        };
        self.cursor += 1;
        Some(Erected::erect_from(&self.inner.data[start..end]))
    }
}

impl FlattenInto<String> for String {
    #[inline]
    fn flatten_into(&self, mut store: Storage<'_>) {
        store.extend(self.bytes());
    }
}

impl ErectFrom<String> for String {
    #[inline]
    fn erect_from(data: &[u8]) -> Self {
        String::from_utf8(data.to_vec()).unwrap()
    }
}

impl FlattenInto<String> for str {
    #[inline]
    fn flatten_into(&self, mut store: Storage<'_>) {
        store.extend(self.bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_pop() {
        let mut names = FlatVec::new();
        names.push("Cerryl");
        names.push(&"Jeslek".to_string());
        assert_eq!(names.pop(), Some("Jeslek".to_string()));
        assert_eq!(names.pop(), Some("Cerryl".to_string()));
        assert_eq!(names.pop::<String>(), None);
    }

    #[test]
    fn iter() {
        let mut names = FlatVec::new();
        names.push(&"Cerryl".to_string());
        names.push(&"Jeslek".to_string());
        let as_vec = names.into_iter::<String>().collect::<Vec<_>>();
        assert_eq!(as_vec, vec!["Cerryl".to_string(), "Jeslek".to_string()]);
    }
}
