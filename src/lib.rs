//! An indirection-collapsing container that generalizes [nested](https://crates.io/crates/nested).
//!
//! A `FlatVec` can be used like a `Vec<String>` or `Vec<Vec<u8>>`, but with a maximum of 2 heap
//! allocations instead of n + 1 (currently it's always 2 but that should change with 1.51).
//!
//! Insertion into and retrieval from a `FlatVec` is mediated by two traits, `IntoFlat` and
//! `FromFlat`, which are both parameterized on two types. The simplest way to use this crate is to
//! `impl IntoFlat<T, u8> for T` and `impl FromFlat<'_, T, u8> for T` for some type `T` in your crate.
//!
//! But since the interface supports a generic backing type parameter, the flattened input objects
//! can be stored as any representation that is convenient. `u8` is a reasonable default choice,
//! but may not be quite right for your application. For example, you may want to write this type
//! alias:
//! ```rust
//! pub type FlatVec<T> = flatvec::FlatVec<T, usize, u8, 3>;
//! ```
//!
//! Additionally, since `FromFlat` has a lifetime parameter, accessing the stored objects in a
//! `FlatVec` can be a zero-copy operation. For example, one may flatten objects with indirections
//! into a dense in-memory storage, then access them later via a reference-wrapping handle type.
//! A simple example of this is in `examples/domain_name.rs`.
//!
//! This interface is extremely powerful and essentially amounts to in-memory serialization and
//! conversion all in one. For example, a user can construct a `FlatVec` that compresses all of its
//! elements with gzip. This is not necessarily a good idea, but you can do it.

#![forbid(unsafe_code)]

use core::{
    convert::{TryFrom, TryInto},
    fmt, iter,
    marker::PhantomData,
    ops::Sub,
    str,
};
use tinyvec::TinyVec;

/// An indirection-collapsing container with minimal allocation
///
/// Read as "An internally-flattening Vec of T, indexed by `IndexTy`, where each `T` is stored as a
/// slice of `BackingTy`"
/// For simple use cases, you may want a type alias for `FlatVec<T, usize, u8, 3>`, but under some
/// workloads it is very profitable to pick a smaller `IndexTy`, possibly even `u8`, and a
/// corresponding inline capacity.
#[derive(Clone)]
pub struct FlatVec<T, IndexTy: Default, BackingTy, const INDEX_INLINE_LEN: usize> {
    data: Box<[BackingTy]>,
    data_len: usize,
    ends: TinyVec<[IndexTy; INDEX_INLINE_LEN]>,
    marker: PhantomData<T>,
}

impl<T, IndexTy, BackingTy, const INDEX_INLINE_LEN: usize> fmt::Debug
    for FlatVec<T, IndexTy, BackingTy, INDEX_INLINE_LEN>
where
    IndexTy: fmt::Debug + Default,
    BackingTy: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("FlatVec")
            .field("data", &&self.data[..self.data_len])
            .field("ends", &self.ends)
            .finish()
    }
}

impl<T, IndexTy, BackingTy, const INDEX_INLINE_LEN: usize> Default
    for FlatVec<T, IndexTy, BackingTy, INDEX_INLINE_LEN>
where
    IndexTy: Default,
{
    #[inline]
    fn default() -> Self {
        Self {
            data: Box::default(),
            data_len: 0,
            ends: TinyVec::default(),
            marker: PhantomData::default(),
        }
    }
}

impl<'a, T: 'a, IndexTy, BackingTy, const INDEX_INLINE_LEN: usize>
    FlatVec<T, IndexTy, BackingTy, INDEX_INLINE_LEN>
where
    IndexTy: Default,
    IndexTy: TryFrom<usize> + Copy + Sub,
    usize: TryFrom<IndexTy>,
    <IndexTy as TryFrom<usize>>::Error: fmt::Debug,
    <usize as TryFrom<IndexTy>>::Error: fmt::Debug,
{
    /// Create a new `FlatVec`, this is just an alias for the `Default` implementation.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of `T` in a `FlatVec<T>`.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.ends.len()
    }

    /// Returns the number of `BackingTy` used to store the elements of a `FlatVec`. This does not
    /// necessarily correlate with storage used to store the indices.
    #[inline]
    #[must_use]
    pub fn data_len(&self) -> usize {
        self.data_len
    }

    #[inline]
    #[must_use]
    pub fn data_capacity(&self) -> usize {
        self.data.len()
    }

    /// Returns true if the len is 0.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ends.len() == 0
    }

    #[inline]
    pub fn clear(&mut self) {
        self.data_len = 0;
        self.ends.clear();
    }

    /// Appends an element to the back of the collection.
    #[inline]
    pub fn push<Source>(&mut self, input: Source)
    where
        Source: IntoFlat<BackingTy, T>,
    {
        input.into_flat(Storage {
            data: &mut self.data,
            data_len: &mut self.data_len,
        });
        self.ends.push(self.data_len.try_into().unwrap());
    }

    /// Construct a `Dest` from the `index`th element's stored representation.
    #[inline]
    #[must_use]
    pub fn get<Dest: 'a>(&'a self, index: usize) -> Option<Dest>
    where
        Dest: FromFlat<'a, BackingTy, T>,
    {
        if index >= self.ends.len() {
            None
        } else {
            let end = self.ends[index].try_into().unwrap();
            let start = if index == 0 {
                0
            } else {
                self.ends[index - 1].try_into().unwrap()
            };
            Some(Dest::from_flat(&self.data[start..end]))
        }
    }

    /// Returns an iterator that constructs a `Dest` from each element's stored representation.
    #[inline]
    pub fn iter<Dest: 'a>(&'a self) -> impl Iterator<Item = Dest> + 'a
    where
        Dest: FromFlat<'a, BackingTy, T>,
    {
        iter::once(0)
            .chain(self.ends.iter().copied().map(|v| v.try_into().unwrap()))
            .zip(self.ends.iter().copied().map(|v| v.try_into().unwrap()))
            .map(move |(start, end)| Dest::from_flat(&self.data[start..end]))
    }
}

impl<'a, T: 'a, IndexTy, BackingTy, const INDEX_INLINE_LEN: usize>
    FlatVec<T, IndexTy, BackingTy, INDEX_INLINE_LEN>
where
    IndexTy: Default,
    IndexTy: TryFrom<usize> + Copy + Sub,
    usize: TryFrom<IndexTy>,
    <IndexTy as TryFrom<usize>>::Error: fmt::Debug,
    <usize as TryFrom<IndexTy>>::Error: fmt::Debug,
    BackingTy: Copy,
{
    /// Removes the `index`th element of a `FlatVec`.
    /// This function is `O(self.len() + self.data_len())`.
    #[inline]
    pub fn remove(&mut self, index: usize) {
        let end: usize = self.ends[index].try_into().unwrap();
        let start = if index == 0 {
            0
        } else {
            self.ends[index - 1].try_into().unwrap()
        };
        self.data.copy_within(end.., start);
        self.ends.remove(index);
        let removed_len = end - start;
        self.data_len -= removed_len;
        self.ends.iter_mut().skip(index).for_each(|end| {
            let change = usize::try_from(*end).unwrap() - removed_len;
            *end = change.try_into().unwrap();
        });
    }
}

// On the surface, this Box juggling seems like a re-implementation of std::vec::Vec.
// The difference is our allocated memory is always default-initialized, so that we can implement
// Storage::allocate, which returns a slice of BackingTy that is not yet used for an object in the
// FlatVec. So just like Vec we have a length and capacity, but unlike Vec the objects beyond the
// container's length are guaranteed to be initialized and thus can be accessed from safe code.
/// A wrapper over the innards of a `FlatVec` which exposes mutating operations which cannot
/// corrupt other elements when inserting a new element.
pub struct Storage<'a, BackingTy> {
    data: &'a mut Box<[BackingTy]>,
    data_len: &'a mut usize,
}

impl<BackingTy> Storage<'_, BackingTy>
where
    BackingTy: Default,
{
    #[inline(never)]
    fn allocate_slow_path(&mut self, requested: usize) {
        let mut data = std::mem::take(self.data).into_vec();
        data.resize_with(
            std::cmp::max(requested + data.len(), 2 * data.len()),
            BackingTy::default,
        );
        *self.data = data.into_boxed_slice();
    }

    /// Returns a `Default` slice of `BackingTy` that will be considered part of this flattened
    /// object.
    ///
    /// Note that even if you do not use part of this slice, the whole slice will be presented to a
    /// `FromFlat` implementation. This function may be called multiple times in a single
    /// `IntoFlat` implementation or combined with `Storage::extend` if a flattened object is
    /// complex, but it is significantly more efficient to use a single `Storage::allocate` call
    /// where possible.
    #[inline]
    pub fn allocate(&mut self, requested: usize) -> &mut [BackingTy] {
        self.reserve(requested);
        let old_len = *self.data_len;
        *self.data_len += requested;
        &mut self.data[old_len..old_len + requested]
    }

    /// Reserves capacity for at least `len` additional `BackingTy`.
    #[inline]
    pub fn reserve(&mut self, requested: usize) {
        if self.data.len() < *self.data_len + requested {
            self.allocate_slow_path(requested);
        }
    }

    /// Inserts the `BackingTy` yielded by `iter`.
    ///
    /// In general, this is ~2x slower than calling `allocate` when the exact size of the inserted
    /// object is known.
    #[inline]
    pub fn extend<Iter>(&mut self, iter: Iter)
    where
        Iter: IntoIterator<Item = BackingTy>,
    {
        let mut iter = iter.into_iter();

        // When the iterator provides a precise size hint, use the allocate interface.
        // This path is ~5x faster than the one below
        let hint = iter.size_hint();
        if Some(hint.0) == hint.1 {
            let data = self.allocate(hint.0);
            for (src, dst) in iter.into_iter().zip(data.iter_mut()) {
                *dst = src;
            }
            return;
        }

        // Insert as many elements as possible into already-allocated space
        for (out, val) in self.data.iter_mut().skip(*self.data_len).zip(&mut iter) {
            *out = val;
            *self.data_len += 1;
        }

        // If there are elements remaining in the iterator, allocate space for them
        if let Some(val) = iter.next() {
            let mut data = std::mem::take(self.data).into_vec();
            data.push(val);
            *self.data_len += 1;

            for val in iter {
                data.push(val);
                *self.data_len += 1;
            }

            if data.capacity() > data.len() {
                data.resize_with(data.capacity(), BackingTy::default);
            }
            let mut data = data.into_boxed_slice();
            std::mem::swap(self.data, &mut data);
            std::mem::forget(data);
        }
    }
}

/// Implement `IntoFlat<Flattened> for Source` to insert a `Source` into a `FlatVec<Flattened>`
pub trait IntoFlat<BackingTy, Flattened> {
    fn into_flat(self, storage: Storage<BackingTy>);
}

/// Implement `FromFlat<'a, Flattened> for Dest` to get a `Dest` from a `FlatVec<Flattened>`
pub trait FromFlat<'a, BackingTy, Flattened> {
    fn from_flat(data: &'a [BackingTy]) -> Self;
}

impl IntoFlat<u8, String> for &str {
    #[inline]
    fn into_flat(self, mut store: Storage<u8>) {
        store
            .allocate(self.as_bytes().len())
            .copy_from_slice(self.as_bytes());
    }
}

impl<'a> FromFlat<'a, u8, String> for &'a str {
    #[inline]
    fn from_flat(data: &'a [u8]) -> &'a str {
        str::from_utf8(&data).unwrap()
    }
}

impl<Iter, BackingTy> IntoFlat<BackingTy, Vec<BackingTy>> for Iter
where
    Iter: IntoIterator<Item = BackingTy>,
    BackingTy: Default,
{
    #[inline]
    fn into_flat(self, mut store: Storage<BackingTy>) {
        store.extend(self);
    }
}

impl<'a, BackingTy> FromFlat<'a, BackingTy, Vec<BackingTy>> for &'a [BackingTy] {
    #[inline]
    fn from_flat(data: &'a [BackingTy]) -> &'a [BackingTy] {
        data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_get() {
        let mut names: FlatVec<String, usize, u8, 3> = FlatVec::new();
        assert!(names.is_empty());
        assert_eq!(names.len(), 0);
        assert_eq!(names.data_len(), 0);

        names.push("Cerryl");
        assert_eq!(names.len(), 1);
        assert!(!names.is_empty());
        assert_eq!(names.data_len(), 6);

        names.push("Jeslek");
        assert_eq!(names.len(), 2);
        assert_eq!(names.data_len(), 12);

        assert_eq!(names.get(0), Some("Cerryl"));
        assert_eq!(names.get(1), Some("Jeslek"));
        assert_eq!(names.get::<&str>(2), None);

        assert_eq!(names.get(0), Some("Cerryl"));
        assert_eq!(names.get(1), Some("Jeslek"));
        assert_eq!(names.get::<&str>(2), None);

        names.clear();
        assert_eq!(names.len(), 0);
        assert!(names.is_empty());
        assert_eq!(names.data_len(), 0);
    }

    #[test]
    fn iter() {
        let mut names: FlatVec<String, usize, u8, 3> = FlatVec::new();
        names.push("Cerryl");
        names.push("Jeslek");
        let as_vec = names.iter().collect::<Vec<&str>>();
        assert_eq!(as_vec, vec!["Cerryl".to_string(), "Jeslek".to_string()]);
    }

    #[test]
    fn remove() {
        let mut places: FlatVec<String, usize, u8, 3> = FlatVec::new();
        places.push("Cyador");
        places.push("Recluce");
        places.push("Hamor");

        assert_eq!(places.get(1), Some("Recluce"));
        assert_eq!(places.get(2), Some("Hamor"));
        places.remove(1);
        assert_eq!(places.get(1), Some("Hamor"));
        assert_eq!(places.get::<&str>(2), None);

        places.clear();
        places.push("Cyador");
        places.push("Recluce");
        places.push("Hamor");

        assert_eq!(places.get(0), Some("Cyador"));
        assert_eq!(places.get(1), Some("Recluce"));
        places.remove(0);
        assert_eq!(places.get(0), Some("Recluce"));
        assert_eq!(places.get(1), Some("Hamor"));
    }

    struct Expander(usize);

    impl IntoFlat<usize, Vec<usize>> for Expander {
        fn into_flat(self, mut storage: Storage<'_, usize>) {
            storage.extend(0..self.0);
        }
    }

    #[test]
    fn storage_extend() {
        let mut data: FlatVec<Vec<usize>, usize, usize, 3> = FlatVec::new();
        data.push(Expander(10));
        assert_eq!(data.data_len(), 10);
        assert_eq!(data.get(0), Some(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9][..]));
        data.push(0..2);
        assert_eq!(data.data_len(), 12);
        assert_eq!(data.get(1), Some(&[0, 1][..]));
    }
}
