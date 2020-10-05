use flatvec::{ErectFrom, FlatVec, FlattenInto, Storage};
use libflate::gzip;

fn main() {
    let mut vec = FlatVec::new();
    let data_to_insert = &b"ffffffffffffffffffffffffffffffffffffffffffffffffffff"[..];
    println!("Original length: {}", data_to_insert.len());
    vec.push(data_to_insert);
    println!("Internal length: {}", vec.data_len());
    let out: Vec<u8> = vec.pop().unwrap();
    assert_eq!(&out, &data_to_insert);
}

struct WriteAdapter<'a>(Storage<'a>);

impl std::io::Write for WriteAdapter<'_> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.0.extend(data.into_iter());
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct CompressedBytes(Vec<u8>);

impl FlattenInto<CompressedBytes> for [u8] {
    fn flatten_into(&self, store: Storage) {
        use std::io::Write;
        let mut encoder = libflate::gzip::Encoder::new(WriteAdapter(store)).unwrap();
        encoder.write_all(&self).unwrap();
        encoder.finish().unwrap();
    }
}

impl ErectFrom<CompressedBytes> for Vec<u8> {
    fn erect_from(data: &[u8]) -> Self {
        use std::io::Read;
        let mut out = Vec::new();
        libflate::gzip::Decoder::new(data)
            .unwrap()
            .read_to_end(&mut out)
            .unwrap();
        out
    }
}
