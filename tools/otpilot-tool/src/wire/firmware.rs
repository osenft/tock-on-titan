use core::convert::TryFrom;

use spiutils::io::Cursor;
use spiutils::protocol::firmware::Message;
use spiutils::protocol::wire::FromWire;
use spiutils::protocol::wire::ToWire;

pub fn serialize<'a, 'b, M: Message<'a> + std::fmt::Debug>
(msg: M, mut buf: &'b mut [u8]) -> &'b [u8] {
    use spiutils::protocol::payload;

    println!("> {:?}", msg);

    let payload_len: u16;
    {
        let mut cursor = Cursor::new(&mut buf[payload::HEADER_LEN..]);

        let header = spiutils::protocol::firmware::Header {
            content: M::TYPE,
        };
        header.to_wire(&mut cursor).expect("failed to write Firmware header");
        msg.to_wire(&mut cursor).expect("failed to write Firmware message");

        payload_len = u16::try_from(cursor.consumed_len())
            .expect("invalid payload length");
    }

    let mut header = payload::Header {
        content: payload::ContentType::Firmware,
        content_len: payload_len,
        checksum: 0,
    };
    header.checksum = payload::compute_checksum(
        &header, &buf[payload::HEADER_LEN..]);

    {
        let mut cursor = Cursor::new(&mut buf);
        header
            .to_wire(&mut cursor)
            .expect("failed to write SpiUtils header");
    }

    let len = spiutils::protocol::payload::HEADER_LEN + payload_len as usize;
    &buf[..len]
}

pub fn deserialize<'a, M: Message<'a> + std::fmt::Debug>
(mut data: &'a [u8]) -> M {
    use spiutils::protocol::payload;

    let spi_header = payload::Header::from_wire(&mut data)
        .expect("SpiUtils header deserialize failed");

    let expected_checksum = payload::compute_checksum(&spi_header, data);
    if spi_header.checksum != expected_checksum {
        panic!("Bad checksum: expected={:x} actual={:x}", expected_checksum, spi_header.checksum);
    }

    if spi_header.content == payload::ContentType::Error {
        let error_header = spiutils::protocol::error::Header::from_wire(&mut data)
            .expect("Error header deserialize failed");

        panic!("Received error message: {:?}", error_header);
    }

    if spi_header.content != payload::ContentType::Firmware {
        panic!("Unexpected SpiUtils header content type: {:?}", spi_header.content);
    }

    data = &data[..spi_header.content_len as usize];

    let header = spiutils::protocol::firmware::Header::from_wire(&mut data)
        .expect("Firmware header deserialize failed");
    if header.content != M::TYPE {
        panic!("Unexpected Firmware header content: {:?}", header.content);
    }

    let msg = M::from_wire(&mut data)
        .expect("Firmware deserialization failed");
    println!("< {:?}", msg);
    msg
}
