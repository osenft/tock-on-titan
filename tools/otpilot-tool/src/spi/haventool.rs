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

use crate::spi::Error;
use crate::spi::Interface;

use std::fs;
use std::io::Read;
use std::process::Command;

#[derive(Debug)]
pub struct Instance {
    tool_path: String,
}

impl Instance {
    pub fn new(tool_path: &str) -> Result<Self, Error> {
        let instance = Instance {
            tool_path: tool_path.to_string(),
        };

        instance.init()?;

        Ok(instance)
    }

    fn init(&self) -> Result<(), Error> {
        self.enter_4b()
    }

    fn enter_4b(&self) -> Result<(), Error> {
        use spiutils::io::Cursor;
        use spiutils::protocol::flash::*;

        let mut buf = [0u8; MAX_HEADER_LEN];

        let header_len: usize =
        {
            let mut cursor = Cursor::new(&mut buf);

            let header = spiutils::protocol::flash::Header::<u32> {
                opcode: OpCode::Enter4ByteAddressMode,
                address: None,
            };
            header.to_wire(&mut cursor).expect("failed to write SPI header");

            cursor.consumed_len()
        };

        self.raw(&buf[..header_len])
    }

    fn raw(&self, data: &[u8]) -> Result<(), Error> {
        // Get a temp file for the output
        let tmp_file = "/tmp/spitmp";

        // Write data into temp file
        if let Err(why) = fs::write(tmp_file, data) {
            return Err(Error::DeviceError(format!("Could not write temp file: {:?}", why)));
        }

        // Execute the command
        let result = self.execute(&[
            "--enter4b=false",
            "--initial_address_size=4",
            "--query_sfdp=false",
            "spi", "raw",
            tmp_file
            ]);
        if let Err(why) = result {
            return Err(why);
        }

        Ok(())
    }

    fn execute<I, S>(&self, args: I) -> Result<(), Error>
    where
    I: IntoIterator<Item = S> + std::fmt::Debug,
    S: AsRef<std::ffi::OsStr> {
        //println!("Executing {} {:?}", self.tool_path, args);

        let maybe_output = Command::new(&self.tool_path)
            .args(args)
            .output();
        if let Err(why) = maybe_output {
            return Err(Error::OperationFailed(format!("Command invocation failed: {:?}", why)));
        }
        let output = maybe_output.unwrap();
        if !output.status.success() {
            return Err(Error::OperationFailed("Non-zero exit code".to_string()));
        }

        Ok(())
    }
}

impl Interface for Instance {
    fn read<'a>(&self, address: u32, buf: &'a mut [u8]) -> Result<&'a [u8], Error> {
        // Get a temp file for the output
        let tmp_file = "/tmp/spitmp";

        // Execute the command
        let result = self.execute(&[
            "--enter4b=false",
            "--initial_address_size=4",
            "--query_sfdp=false",
            "spi", "read",
            "--start", &address.to_string(),
            "--length", &buf.len().to_string(),
            tmp_file
            ]);
        if let Err(why) = result {
            return Err(why);
        }

        // Read the temp file into memory
        let maybe_file = fs::File::open(tmp_file);
        if let Err(why) = maybe_file {
            return Err(Error::DeviceError(format!("Could not open temp file: {:?}", why)));
        }
        let mut file = maybe_file.unwrap();
        let maybe_size = file.read(buf);
        if let Err(why) = maybe_size {
            return Err(Error::DeviceError(format!("Could not read temp file: {:?}", why)));
        }

        Ok(&buf[..maybe_size.unwrap()])
    }

    fn write(&self, address: u32, data: &[u8]) -> Result<(), Error> {
        // Get a temp file for the output
        let tmp_file = "/tmp/spitmp";

        // Write data into temp file
        if let Err(why) = fs::write(tmp_file, data) {
            return Err(Error::DeviceError(format!("Could not write temp file: {:?}", why)));
        }

        // Execute the command
        let result = self.execute(&[
            "--enter4b=false",
            "--initial_address_size=4",
            "--query_sfdp=false",
            "spi", "write",
            "--start", &address.to_string(),
            "--length", &data.len().to_string(),
            tmp_file
            ]);
        if let Err(why) = result {
            return Err(why);
        }

        Ok(())
    }
}
