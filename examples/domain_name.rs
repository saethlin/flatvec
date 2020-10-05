use flatvec::{ErectFrom, FlatVec, FlattenInto, Storage};

fn main() {
    let mut names = FlatVec::new();
    names.push(&DomainNameRef {
        ttl: 60,
        time_seen: 31415,
        name: &b"google.com"[..],
    });
    assert_eq!(
        names.pop(),
        Some(DomainName {
            ttl: 60,
            time_seen: 31415,
            name: b"google.com".to_vec()
        })
    );
}

#[derive(PartialEq, Eq, Debug)]
pub struct DomainName {
    ttl: u32,
    time_seen: u32,
    name: Vec<u8>,
}

#[derive(PartialEq, Eq, Debug)]
pub struct DomainNameRef<'a> {
    ttl: u32,
    time_seen: u32,
    name: &'a [u8],
}

impl ErectFrom<DomainName> for DomainName {
    fn erect_from(data: &[u8]) -> Self {
        Self {
            time_seen: u32::from_ne_bytes([data[0], data[1], data[2], data[3]]),
            ttl: u32::from_ne_bytes([data[4], data[5], data[6], data[7]]),
            name: data[8..].to_vec(),
        }
    }
}

impl FlattenInto<DomainName> for DomainNameRef<'_> {
    fn flatten_into(&self, mut store: Storage) {
        store.reserve(self.name.len() + 8);
        store.extend(&self.time_seen.to_ne_bytes());
        store.extend(&self.ttl.to_ne_bytes());
        store.extend(self.name.iter().cloned());
    }
}