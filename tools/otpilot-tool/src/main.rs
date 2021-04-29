// Copyright 2021 lowRISC contributors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

mod spi;
mod wire;

use clap::App;
use clap::AppSettings;
use clap::Arg;
use clap::SubCommand;

use spiutils::compat::firmware::BuildInfo;
use spiutils::protocol::wire::FromWire;

use std::cmp::min;
use std::fs::OpenOptions;
use std::io::Read as _;

const HAVENTOOL_DEFAULT_MAILBOX_ADDR: u32 = 0x80000;
const SPI_MAX_WRITE: usize = 512;
const SPI_MAX_READ: usize = 2048;
const FIRMWARE_INFO_OFFSET: usize = 860;

struct Device<'a> {
    spi: &'a dyn spi::Interface,
    mailbox_addr: u32,
}

impl std::fmt::Debug for Device<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Device {{ spi: ?, mailbox_addr: {:#x} }}", self.mailbox_addr)
    }
}

impl <'a> Device<'a> {
    pub fn new(spi: &'a dyn spi::Interface, mailbox_addr: u32) -> Device<'a> {
        Device {
            spi,
            mailbox_addr,
        }
    }

    fn write_mailbox(&self, data: &[u8]) {
        self.spi.write(self.mailbox_addr, data).expect("Mailbox write failed");
    }

    fn read_mailbox(&self) -> [u8; SPI_MAX_READ] {
        let mut buf = [0u8; SPI_MAX_READ];
        self.spi.read(self.mailbox_addr, &mut buf).expect("SPI read failed");
        buf
    }

    fn send_manticore<'m, M: manticore::protocol::Request<'m>>(&self, msg: M) {
        let mut buf = [0u8; SPI_MAX_WRITE];
        let send_buf = wire::manticore::serialize(msg, &mut buf);
        self.write_mailbox(send_buf);
    }

    fn send_firmware<'m, M: spiutils::protocol::firmware::Message<'m> + std::fmt::Debug>(&self, msg: M) {
        let mut buf = [0u8; SPI_MAX_WRITE];
        let send_buf = wire::firmware::serialize(msg, &mut buf);
        self.write_mailbox(send_buf);
    }

    pub fn device_info(&self) {
        use manticore::protocol::device_info::*;

        self.send_manticore(DeviceInfoRequest {
            index: manticore::protocol::device_info::InfoIndex::UniqueChipIndex,
        });
        let buf = self.read_mailbox();
        let resp = wire::manticore::deserialize::<DeviceInfoResponse>(&buf);

        println!("Response: {:?}", resp);
    }

    pub fn fw_info(&self, index: u8) {
        use manticore::protocol::firmware_version::*;

        self.send_manticore(FirmwareVersionRequest {
            index,
        });
        let buf = self.read_mailbox();
        let resp = wire::manticore::deserialize::<FirmwareVersionResponse>(&buf);

        println!("Response: {:?}", resp);

        match index {
            0 => println!("Version: '{}'", std::str::from_utf8(resp.version).expect("Could not UTF-8 decode version")),
            1 => println!("RO: {:?}", wire::spiutils::deserialize::<BuildInfo>(resp.version)),
            2 => println!("RW: {:?}", wire::spiutils::deserialize::<BuildInfo>(resp.version)),
            _ => (),
        }
    }

    fn firmware_get_inactive_ro(&self) -> spiutils::driver::firmware::SegmentInfo {
        use spiutils::protocol::firmware::*;

        self.send_firmware(InactiveSegmentsInfoRequest {});
        let buf = self.read_mailbox();
        let resp = wire::firmware::deserialize::<InactiveSegmentsInfoResponse>(&buf);

        resp.ro
    }

    fn firmware_get_inactive_rw(&self) -> spiutils::driver::firmware::SegmentInfo {
        use spiutils::protocol::firmware::*;

        self.send_firmware(InactiveSegmentsInfoRequest {});
        let buf = self.read_mailbox();
        let resp = wire::firmware::deserialize::<InactiveSegmentsInfoResponse>(&buf);

        resp.rw
    }

    fn firmware_update_prepare(&self, segment_and_location: spiutils::protocol::firmware::SegmentAndLocation) -> u16 {
        use spiutils::protocol::firmware::*;

        self.send_firmware(UpdatePrepareRequest {
            segment_and_location,
        });
        let buf = self.read_mailbox();
        let resp = wire::firmware::deserialize::<UpdatePrepareResponse>(&buf);

        if resp.segment_and_location != segment_and_location {
            panic!("Invalid UpdatePrepareResponse::segment_and_location={:?}", resp.segment_and_location);
        }
        if resp.result != UpdatePrepareResult::Success {
            panic!("Invalid UpdatePrepareResponse::result={:?}", resp.result);
        }
        resp.max_chunk_length
    }

    fn firmware_write_chunk(&self,
        segment_and_location: spiutils::protocol::firmware::SegmentAndLocation,
        offset: u32,
        data: &[u8]) {
        use spiutils::protocol::firmware::*;

        self.send_firmware(WriteChunkRequest {
            segment_and_location,
            offset,
            data,
        });
        let buf = self.read_mailbox();
        let resp = wire::firmware::deserialize::<WriteChunkResponse>(&buf);

        if resp.segment_and_location != segment_and_location {
            panic!("Invalid WriteChunkResponse::segment_and_location={:?}", resp.segment_and_location);
        }
        if resp.offset != offset {
            panic!("Invalid WriteChunkResponse::offset={:?}", resp.offset);
        }
        if resp.result != WriteChunkResult::Success {
            panic!("Invalid WriteChunkResponse::result={:?}", resp.result);
        }
    }

    fn fw_update(&self, segment: spiutils::driver::firmware::SegmentInfo, file_name: &str) {
        let mut file = OpenOptions::new()
            .read(true)
            .open(&file_name)
            .expect(format!("failed to open {:?} file", segment.identifier).as_str());

        let mut file_buf = Vec::new();
        file
            .read_to_end(&mut file_buf)
            .expect(format!("couldn't read from {:?} file", segment.identifier).as_str());

        if file_buf.len() > segment.size as usize {
            panic!("File {:?} has size {} but should be {}", segment.identifier, file_buf.len(), segment.size);
        }

        let max_chunk_length = self.firmware_update_prepare(segment.identifier);

        let mut pos = 0u32;
        let data = file_buf.as_slice();
        let data_len = data.len() as u32;
        while pos < data_len {
            let chunk_len = min(max_chunk_length as u32, data_len - pos) as u16;

            if chunk_len == 0 {
                panic!("Invalid chunk len");
            }

            let end_pos: usize = pos as usize + chunk_len as usize;
            self.firmware_write_chunk(segment.identifier, pos, &data[pos as usize..end_pos]);

            pos += chunk_len as u32;
        }
    }

    pub fn ro_update(&self, a_file: &str, b_file: &str) {
        use spiutils::protocol::firmware::*;

        let inactive = self.firmware_get_inactive_ro();

        let file_name = match inactive.identifier {
            SegmentAndLocation::RoA => a_file,
            SegmentAndLocation::RoB => b_file,
            sal => panic!("Unexpected inactive segment/location {:?}", sal),
        };

        self.fw_update(inactive, file_name);
    }

    pub fn rw_update(&self, a_file: &str, b_file: &str) {
        use spiutils::protocol::firmware::*;

        let inactive = self.firmware_get_inactive_rw();

        let file_name = match inactive.identifier {
            SegmentAndLocation::RwA => a_file,
            SegmentAndLocation::RwB => b_file,
            sal => panic!("Unexpected inactive segment/location {:?}", sal),
        };

        self.fw_update(inactive, file_name);
    }

    pub fn build_info(&self, filename: &str) {
        let mut file = OpenOptions::new()
            .read(true)
            .open(&filename)
            .expect(format!("failed to open file").as_str());

        let mut buf = Vec::new();
        file
            .read_to_end(&mut buf)
            .expect(format!("couldn't read from file").as_str());

        let build_info = spiutils::compat::firmware::BuildInfo::from_wire(&mut buf[FIRMWARE_INFO_OFFSET..])
            .expect("BuildInfo deserialize failed");

        println!("BuildInfo: {:?}", build_info);
    }

    pub fn inactive_segments_info(&self) {
        use spiutils::protocol::firmware::*;

        self.send_firmware(InactiveSegmentsInfoRequest {});
        let buf = self.read_mailbox();
        let resp = wire::firmware::deserialize::<InactiveSegmentsInfoResponse>(&buf);

        println!("Inactive RO: {:?}", resp.ro);
        println!("Inactive RW: {:?}", resp.rw);
    }

    pub fn reboot(&self) {
        use spiutils::protocol::firmware::*;

        self.send_firmware(RebootRequest {
            time: RebootTime::Immediate,
        });
        let buf = self.read_mailbox();
        let resp = wire::firmware::deserialize::<RebootResponse>(&buf);

        println!("Response: {:?}", resp);
    }
}

fn main() {
    let default_mailbox_addr_str_haventool = format!("{:x}", HAVENTOOL_DEFAULT_MAILBOX_ADDR);
    let default_mailbox_addr_str_spidevice = format!("{:x}", 0);
    let app = App::new("OTPilot Tool")
        .version("0.1")
        .author("lowRISC contributors")
        .about("Command line tool for OTPilot")
        .setting(AppSettings::ArgRequiredElseHelp)
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .arg(
            Arg::with_name("v")
                .short("v")
                .long("verbose")
                .multiple(true)
                .help("Sets the level of verbosity"),
        )
        .arg(
            Arg::with_name("haventool")
                .short("h")
                .long("haventool")
                .help("Path to haventool for SPI communication")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("spidevice")
                .short("s")
                .long("spidevice")
                .help("Path to SPI device file")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("mailbox_addr")
                .short("m")
                .long("mailbox_addr")
                .help("Mailbox address (in hex format) relative to the SPI communication device")
                .takes_value(true)
                .default_value_if("haventool", None, default_mailbox_addr_str_haventool.as_str())
                .default_value_if("spidevice", None, default_mailbox_addr_str_spidevice.as_str()),
        )
        .subcommand(
            SubCommand::with_name("device_info")
                .about("Get device information")
        )
        .subcommand(
            SubCommand::with_name("fw_info")
                .about("Get firmware information")
                .arg(
                    Arg::with_name("index")
                        .short("i")
                        .long("index")
                        .help("Firmware version index to get")
                        .required(false)
                        .takes_value(true)
                        .default_value("0"),
                )
        )
        .subcommand(
            SubCommand::with_name("build_info")
                .about("Get build information from file")
                .arg(
                    Arg::with_name("file")
                        .short("f")
                        .long("file")
                        .help("file containing firmware binary")
                        .required(false)
                        .takes_value(true),
                )
        )
        .subcommand(
            SubCommand::with_name("inactive_segments_info")
                .about("Get information on inactive firmware segments")
        )
        .subcommand(
            SubCommand::with_name("ro_update")
                .about("Update RO")
                .arg(
                    Arg::with_name("image_a")
                        .short("a")
                        .long("image_a")
                        .help("file containing RO-A")
                        .required(true)
                        .takes_value(true),
                )
                .arg(
                    Arg::with_name("image_b")
                        .short("b")
                        .long("image_b")
                        .help("file containing RO-B")
                        .required(true)
                        .takes_value(true),
                ),
        )
        .subcommand(
            SubCommand::with_name("rw_update")
                .about("Update RW")
                .arg(
                    Arg::with_name("image_a")
                        .short("a")
                        .long("image_a")
                        .help("file containing RW-A")
                        .required(true)
                        .takes_value(true),
                )
                .arg(
                    Arg::with_name("image_b")
                        .short("b")
                        .long("image_b")
                        .help("file containing RW-B")
                        .required(true)
                        .takes_value(true),
                )
        )
        .subcommand(
            SubCommand::with_name("reboot")
                .about("Reboot device")
        );
    let matches = app.get_matches();

    let spi: &dyn spi::Interface;
    let haventool: Option<spi::haventool::Instance>;
    let spidevice: Option<spi::device::Instance>;

    let haventool_arg = matches.value_of("haventool");
    let spidevice_arg = matches.value_of("spidevice");
    if haventool_arg.is_some() {
        match spi::haventool::Instance::new(haventool_arg.unwrap()) {
            Ok(instance) => {
                haventool = Some(instance);
                spi = haventool.as_ref().unwrap();
            },
            Err(why) => panic!("Cannot instantiate Haventool: {:?}", why),
        }
    } else if spidevice_arg.is_some() {
        spidevice = Some(spi::device::Instance::new(spidevice_arg.unwrap()));
        spi = spidevice.as_ref().unwrap();
    } else {
        panic!("Must specify SPI interface");
    }

    let mailbox_addr = u32::from_str_radix(matches.value_of("mailbox_addr").unwrap(), 16)
        .expect("Could not parse mailbox_addr");

    let device = Device::new(spi, mailbox_addr);

    println!("{:?}", device);

    if let Some(subcommand_matches) = matches.subcommand_matches("ro_update") {
        device.ro_update(
            subcommand_matches.value_of("image_a").unwrap(),
            subcommand_matches.value_of("image_b").unwrap(),
        );
    }
    else if let Some(subcommand_matches) = matches.subcommand_matches("rw_update") {
        device.rw_update(
            subcommand_matches.value_of("image_a").unwrap(),
            subcommand_matches.value_of("image_b").unwrap(),
        );
    }
    else if let Some(subcommand_matches) = matches.subcommand_matches("fw_info") {
        let index = u8::from_str_radix(subcommand_matches.value_of("index").unwrap(), 10)
            .expect("Could not parse index");
        device.fw_info(index);
    }
    else if let Some(_) = matches.subcommand_matches("device_info") {
        device.device_info();
    }
    else if let Some(_) = matches.subcommand_matches("inactive_segments_info") {
        device.inactive_segments_info();
    }
    else if let Some(subcommand_matches) = matches.subcommand_matches("build_info") {
        device.build_info(subcommand_matches.value_of("file").unwrap());
    }
    else if let Some(_) = matches.subcommand_matches("reboot") {
        device.reboot();
    }
}
