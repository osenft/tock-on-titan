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

pub mod device;
pub mod haventool;

/// Error definitions
#[derive(Debug)]
pub enum Error {
    /// The underlying device could not be opened / accessed.
    DeviceError(String),

    /// The operation failed with a human readable description.
    OperationFailed(String),
}

/// The SPI interface definition.
pub trait Interface {
    /// Read bytes from a SPI interface `len` bytes starting at `address`.
    fn read<'a>(&self, address: u32, buf: &'a mut [u8]) -> Result<&'a [u8], Error>;

    /// Write bytes to a SPI interface at `address`.
    fn write(&self, address: u32, data: &[u8]) -> Result<(), Error>;
}

impl std::fmt::Debug for dyn Interface {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Interface{{}}")
    }
}
