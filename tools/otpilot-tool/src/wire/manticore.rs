use core::convert::TryFrom;

use manticore::io::Cursor as ManticoreCursor;
use manticore::protocol::Request as ManticoreRequest;
use manticore::protocol::Response as ManticoreResponse;
use manticore::protocol::wire::FromWire as ManticoreFromWire;
use manticore::protocol::wire::ToWire as ManticoreToWire;

use spiutils::io::Cursor as SpiutilsCursor;
use spiutils::protocol::wire::FromWire as SpiutilsFromWire;
use spiutils::protocol::wire::ToWire as SpiutilsToWire;

pub fn serialize<'a, 'b, M: ManticoreRequest<'a>>
(msg: M, mut buf: &'b mut [u8]) -> &'b [u8] {
    use spiutils::protocol::payload;

    let payload_len: u16;
    {
        let mut cursor = ManticoreCursor::new(&mut buf[payload::HEADER_LEN..]);

        let header = manticore::protocol::Header {
            is_request: true,
            command: M::TYPE,
        };
        header.to_wire(&mut cursor).expect("failed to write Manticore header");
        msg.to_wire(&mut cursor).expect("failed to write Manticore request");

        payload_len = u16::try_from(cursor.consumed_len())
            .expect("invalid payload length");
    }

    let mut header = payload::Header {
        content: payload::ContentType::Manticore,
        content_len: payload_len,
        checksum: 0,
    };
    header.checksum = payload::compute_checksum(
        &header, &buf[payload::HEADER_LEN..]);

    {
        let mut cursor = SpiutilsCursor::new(&mut buf);
        header
            .to_wire(&mut cursor)
            .expect("failed to write spiutils header");
    }

    let len = payload::HEADER_LEN + payload_len as usize;
    &buf[..len]
}

pub fn deserialize<'a, M: ManticoreResponse<'a>>
(mut data: &'a [u8]) -> M {
    use spiutils::protocol::payload;

    let orig_data = data;
    let spi_header = match payload::Header::from_wire(&mut data) {
        Ok(val) => val,
        Err(why) => panic!("SpiUtils header deserialize failed: {:?}. Buf={:?}", why, orig_data),
    };

    let expected_checksum = payload::compute_checksum(&spi_header, data);
    if spi_header.checksum != expected_checksum {
        panic!("Bad checksum: expected={:x} actual={:x}", expected_checksum, spi_header.checksum);
    }

    if spi_header.content == payload::ContentType::Error {
        let error_header = spiutils::protocol::error::Header::from_wire(&mut data)
            .expect("Error header deserialize failed");

        panic!("Received error message: {:?}", error_header);
    }

    if spi_header.content != payload::ContentType::Manticore {
        panic!("Unexpected Spiutils header content type: {:?}", spi_header.content);
    }

    data = &data[..spi_header.content_len as usize];

    let header = manticore::protocol::Header::from_wire(&mut data)
        .expect("Manticore header deserialize failed");
    if header.command != M::TYPE {
        panic!("Unexpected Manticore header command: {:?}", header.command);
    }
    if header.is_request {
        panic!("Unexpected Manticore header is_request: {}", header.is_request);
    }

    M::from_wire(&mut data)
        .expect("Manticore deserialization failed")
}
