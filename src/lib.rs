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

impl<'a, T: 'a> FlatVec<T> {
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
    pub fn remove(&mut self, index: usize) {
        let end = self.ends[index];
        let start = if index == 0 { 0 } else { self.ends[index - 1] };
        self.data.drain(start..end);
        self.ends.remove(index);
        let len = end - start;
        self.ends.iter_mut().skip(index).for_each(|end| *end -= len);
    }

    #[inline]
    pub fn push<Source>(&mut self, input: Source)
    where
        Source: IntoFlat<T>,
    {
        input.into_flat(&mut Storage(&mut self.data));
        self.ends.push(self.data.len());
    }

    #[inline]
    pub fn get<Dest: 'a>(&'a self, index: usize) -> Option<Dest>
    where
        Dest: FromFlat<'a, T>,
    {
        let end = *self.ends.get(index)?;
        let start = if index == 0 { 0 } else { self.ends[index - 1] };
        Some(Dest::from_flat(&self.data[start..end]))
    }

    #[inline]
    pub fn iter<Dest: 'a>(&'a self) -> impl Iterator<Item = Dest> + 'a
    where
        Dest: FromFlat<'a, T>,
    {
        std::iter::once(&0usize)
            .chain(self.ends.iter())
            .zip(self.ends.iter())
            .map(move |(&start, &end)| Dest::from_flat(&self.data[start..end]))
    }
}

/// A wrapper over the innards of a `FlatVec` which exposes mutating operations which cannot
/// corrupt other elements during a push
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

pub trait IntoFlat<Flattened> {
    fn into_flat(self, storage: &mut Storage);
}

pub trait FromFlat<'a, Flattened> {
    fn from_flat(data: &'a [u8]) -> Self;
}

impl IntoFlat<String> for String {
    #[inline]
    fn into_flat(self, store: &mut Storage) {
        store.extend(self.bytes());
    }
}

impl FromFlat<'_, String> for String {
    #[inline]
    fn from_flat(data: &[u8]) -> Self {
        String::from_utf8(data.to_vec()).unwrap()
    }
}

impl IntoFlat<String> for &str {
    #[inline]
    fn into_flat(self, store: &mut Storage) {
        store.extend(self.bytes());
    }
}

impl<'a> FromFlat<'a, String> for &'a str {
    #[inline]
    fn from_flat(data: &'a [u8]) -> &'a str {
        std::str::from_utf8(&data).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_get() {
        let mut names = FlatVec::new();
        names.push("Cerryl");
        names.push("Jeslek".to_string());
        assert_eq!(names.get(0), Some("Cerryl"));
        assert_eq!(names.get(1), Some("Jeslek"));
        assert_eq!(names.get::<String>(2), None);
    }

    #[test]
    fn iter() {
        let mut names = FlatVec::new();
        names.push("Cerryl".to_string());
        names.push("Jeslek".to_string());
        let as_vec = names.iter::<String>().collect::<Vec<_>>();
        assert_eq!(as_vec, vec!["Cerryl".to_string(), "Jeslek".to_string()]);
    }
}
