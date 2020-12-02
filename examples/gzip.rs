use flatvec::{FlatVec, FromFlat, IntoFlat, Storage};

fn main() {
    let mut vec: FlatVec<CompressedBytes, _> = FlatVec::default();
    let data_to_insert = &b"ffffffffffffffffffffffffffffffffffffffffffffffffffff"[..];
    println!("Original length: {}", data_to_insert.len());
    vec.push(data_to_insert);
    println!("Internal length: {}", vec.data_len());
    let out: Vec<u8> = vec.get(0).unwrap();
    assert_eq!(&out, &data_to_insert);
}

struct WriteAdapter<'a>(Storage<'a>);

impl std::io::Write for WriteAdapter<'_> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.0.extend(data.iter());
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct CompressedBytes(Vec<u8>);

impl IntoFlat<CompressedBytes> for &[u8] {
    fn into_flat(self, store: Storage) {
        use std::io::Write;
        let mut encoder = libflate::gzip::Encoder::new(WriteAdapter(store)).unwrap();
        encoder.write_all(&self).unwrap();
        encoder.finish().unwrap();
    }
}

impl FromFlat<'_, CompressedBytes> for Vec<u8> {
    fn from_flat(data: &[u8]) -> Self {
        use std::io::Read;
        let mut out = Vec::new();
        libflate::gzip::Decoder::new(data)
            .unwrap()
            .read_to_end(&mut out)
            .unwrap();
        out
    }
}
