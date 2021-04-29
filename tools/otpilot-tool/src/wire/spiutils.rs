use spiutils::protocol::wire::FromWire;

pub fn deserialize<'a, M: FromWire<'a> + std::fmt::Debug>
(mut data: &'a [u8]) -> M {
    M::from_wire(&mut data)
        .expect("FromWire deserialization failed")
}
