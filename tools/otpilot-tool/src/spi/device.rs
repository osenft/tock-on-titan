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

use std::fs::File;
use std::fs::OpenOptions;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;

#[derive(Debug)]
pub struct Instance {
    #[allow(dead_code)]
    device_path: String,
}

impl Instance {
    pub fn new(device_path: &str) -> Instance {
        Instance {
            device_path: device_path.to_string(),
        }
    }
}

impl Interface for Instance {
    fn read<'a>(&self, address: u32, buf: &'a mut [u8]) -> Result<&'a [u8], Error> {
        let mut file = match File::open(&self.device_path) {
            Ok(file) => file,
            Err(why) => return Err(Error::DeviceError(format!("Could not open device file: {:?}", why))),
        };
        if let Err(why) = file.seek(SeekFrom::Start(address.into())) {
            return Err(Error::DeviceError(format!("Could not seek into device file: {:?}", why)))
        }

        let maybe_size = file.read(buf);
        if let Err(why) = maybe_size {
            return Err(Error::DeviceError(format!("Could not read from device file: {:?}", why)));
        }

        Ok(&buf[..maybe_size.unwrap()])
    }

    fn write(&self, address: u32, data: &[u8]) -> Result<(), Error> {
        let mut file = match OpenOptions::new().write(true).open(&self.device_path) {
            Ok(file) => file,
            Err(why) => return Err(Error::DeviceError(format!("Could not open device file: {:?}", why))),
        };
        if let Err(why) = file.seek(SeekFrom::Start(address.into())) {
            return Err(Error::DeviceError(format!("Could not seek into device file: {:?}", why)))
        }

        if let Err(why) = file.write(data) {
            return Err(Error::DeviceError(format!("Could not write to device file: {:?}", why)))
        }

        if let Err(why) = file.flush() {
            return Err(Error::DeviceError(format!("Could not flush device file: {:?}", why)))
        }

        Ok(())
    }
}
