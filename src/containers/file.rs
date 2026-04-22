// RGB ops library for working with smart contracts on Bitcoin & Lightning
//
// SPDX-License-Identifier: Apache-2.0
//
// Written in 2019-2024 by
//     Dr Maxim Orlovsky <orlovsky@lnp-bp.org>
//
// Copyright (C) 2019-2024 LNP/BP Standards Association. All rights reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::fmt::{self, Debug, Display, Formatter};
use std::io::{self, Read, Write};

use amplify::confinement::U32 as FILE_MAX_LEN;
use armor::{AsciiArmor, StrictArmor};
#[cfg(all(feature = "fs", feature = "serde"))]
use strict_encoding::StrictReader;
use strict_encoding::{StreamReader, StreamWriter, StrictDecode, StrictEncode};

#[cfg(all(feature = "fs", feature = "serde"))]
use crate::containers::{Consignment, ValidConsignment};
use crate::containers::{Contract, Kit, Transfer};

const RGB_PREFIX: [u8; 4] = *b"RGB\x00";
const MAGIC_LEN: usize = 3;

#[derive(Debug, Display, Error, From)]
#[display(doc_comments)]
pub enum LoadError {
    /// invalid file data.
    InvalidMagic,

    #[display(inner)]
    #[from]
    #[from(io::Error)]
    Decode(strict_encoding::DecodeError),

    #[display(inner)]
    #[from]
    Armor(armor::StrictArmorError),

    #[cfg(all(feature = "fs", feature = "serde"))]
    #[display(inner)]
    #[from]
    Json(serde_json::Error),
}

pub trait FileContent: StrictArmor {
    /// Magic bytes used in saving/restoring container from a file.
    const MAGIC: [u8; MAGIC_LEN];

    fn load(mut data: impl Read) -> Result<Self, LoadError> {
        let mut rgb = [0u8; 4];
        let mut magic = [0u8; MAGIC_LEN];
        data.read_exact(&mut rgb)?;
        data.read_exact(&mut magic)?;
        if rgb != RGB_PREFIX || magic != Self::MAGIC {
            return Err(LoadError::InvalidMagic);
        }

        let reader = StreamReader::new::<FILE_MAX_LEN>(data);
        let me = Self::strict_read(reader)?;

        Ok(me)
    }

    fn save(&self, mut writer: impl Write) -> Result<(), io::Error> {
        writer.write_all(&RGB_PREFIX)?;
        writer.write_all(&Self::MAGIC)?;

        let writer = StreamWriter::new::<FILE_MAX_LEN>(writer);
        self.strict_write(writer)?;

        Ok(())
    }

    #[cfg(feature = "fs")]
    fn load_file(path: impl AsRef<std::path::Path>) -> Result<Self, LoadError> {
        let file = std::fs::File::open(path)?;
        Self::load(file)
    }

    #[cfg(feature = "fs")]
    fn save_file(&self, path: impl AsRef<std::path::Path>) -> Result<(), io::Error> {
        let file = std::fs::File::create(path)?;
        self.save(file)
    }

    #[cfg(feature = "fs")]
    fn load_armored(path: impl AsRef<std::path::Path>) -> Result<Self, LoadError> {
        let armor = std::fs::read_to_string(path)?;
        let content = Self::from_ascii_armored_str(&armor)?;
        Ok(content)
    }

    #[cfg(feature = "fs")]
    fn save_armored(&self, path: impl AsRef<std::path::Path>) -> Result<(), io::Error> {
        std::fs::write(path, self.to_ascii_armored_string())
    }
}

impl FileContent for Kit {
    const MAGIC: [u8; MAGIC_LEN] = *b"KIT";
}

impl FileContent for Contract {
    const MAGIC: [u8; MAGIC_LEN] = *b"CON";
}

impl FileContent for Transfer {
    const MAGIC: [u8; MAGIC_LEN] = *b"TFR";
}

#[cfg(all(feature = "fs", feature = "serde"))]
impl<const TRANSFER: bool> ValidConsignment<TRANSFER> {
    const VALID_MAGIC: [u8; MAGIC_LEN] = if TRANSFER { *b"VTF" } else { *b"VCO" };

    pub fn save_file(&self, path: impl AsRef<std::path::Path>) -> Result<(), io::Error> {
        let mut file = std::fs::File::create(path)?;
        file.write_all(&RGB_PREFIX)?;
        file.write_all(&Self::VALID_MAGIC)?;

        let writer = StreamWriter::new::<FILE_MAX_LEN>(&mut file);
        StrictEncode::strict_write(&**self, writer)?;

        serde_json::to_writer(&mut file, self.validation_status()).map_err(io::Error::other)?;
        Ok(())
    }

    pub fn load_file(path: impl AsRef<std::path::Path>) -> Result<Self, LoadError> {
        let mut file = std::fs::File::open(path)?;
        let mut rgb = [0u8; 4];
        let mut magic = [0u8; MAGIC_LEN];
        file.read_exact(&mut rgb)?;
        file.read_exact(&mut magic)?;
        if rgb != RGB_PREFIX || magic != Self::VALID_MAGIC {
            return Err(LoadError::InvalidMagic);
        }

        let consignment = {
            let stream = StreamReader::new::<FILE_MAX_LEN>(&mut file);
            let mut reader = StrictReader::with(stream);
            Consignment::<TRANSFER>::strict_decode(&mut reader)?
        };

        let validation_status = serde_json::from_reader(&mut file)?;

        Ok(Self::from_parts(consignment, validation_status))
    }
}

#[derive(Clone, Debug, From)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "serde_crate", rename_all = "camelCase", tag = "type")
)]
pub enum UniversalFile {
    #[from]
    Kit(Kit),

    #[from]
    Contract(Contract),

    #[from]
    Transfer(Transfer),
}

impl UniversalFile {
    pub fn load(mut data: impl Read) -> Result<Self, LoadError> {
        let mut rgb = [0u8; 4];
        let mut magic = [0u8; MAGIC_LEN];
        data.read_exact(&mut rgb)?;
        data.read_exact(&mut magic)?;
        if rgb != RGB_PREFIX {
            return Err(LoadError::InvalidMagic);
        }
        let mut reader = StreamReader::new::<FILE_MAX_LEN>(data);
        Ok(match magic {
            x if x == Kit::MAGIC => Kit::strict_read(&mut reader)?.into(),
            x if x == Contract::MAGIC => Contract::strict_read(&mut reader)?.into(),
            x if x == Transfer::MAGIC => Transfer::strict_read(&mut reader)?.into(),
            _ => return Err(LoadError::InvalidMagic),
        })
    }

    pub fn save(&self, mut writer: impl Write) -> Result<(), io::Error> {
        writer.write_all(&RGB_PREFIX)?;
        let magic = match self {
            UniversalFile::Kit(_) => Kit::MAGIC,
            UniversalFile::Contract(_) => Contract::MAGIC,
            UniversalFile::Transfer(_) => Transfer::MAGIC,
        };
        writer.write_all(&magic)?;

        let writer = StreamWriter::new::<FILE_MAX_LEN>(writer);

        match self {
            UniversalFile::Kit(content) => content.strict_write(writer),
            UniversalFile::Contract(content) => content.strict_write(writer),
            UniversalFile::Transfer(content) => content.strict_write(writer),
        }
    }

    #[cfg(feature = "fs")]
    pub fn load_file(path: impl AsRef<std::path::Path>) -> Result<Self, LoadError> {
        let file = std::fs::File::open(path)?;
        Self::load(file)
    }

    #[cfg(feature = "fs")]
    pub fn save_file(&self, path: impl AsRef<std::path::Path>) -> Result<(), io::Error> {
        let file = std::fs::File::create(path)?;
        self.save(file)
    }
}

impl Display for UniversalFile {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            UniversalFile::Kit(content) => Display::fmt(&content.display_ascii_armored(), f),
            UniversalFile::Contract(content) => Display::fmt(&content.display_ascii_armored(), f),
            UniversalFile::Transfer(content) => Display::fmt(&content.display_ascii_armored(), f),
        }
    }
}

#[cfg(test)]
mod test {
    use std::fs::OpenOptions;
    use std::str::FromStr;

    use rgb::validation;

    use super::*;
    use crate::containers::ValidTransfer;

    static DEFAULT_KIT_PATH: &str = "asset/kit.default";
    #[cfg(feature = "fs")]
    static ARMORED_KIT_PATH: &str = "asset/armored_kit.default";

    static DEFAULT_CONTRACT_PATH: &str = "asset/contract.default";
    #[cfg(feature = "fs")]
    static ARMORED_CONTRACT_PATH: &str = "asset/armored_contract.default";

    static DEFAULT_TRANSFER_PATH: &str = "asset/transfer.default";
    #[cfg(feature = "fs")]
    static ARMORED_TRANSFER_PATH: &str = "asset/armored_transfer.default";

    static DEFAULT_VALID_TRANSFER_PATH: &str = "asset/valid_transfer.default";

    #[test]
    fn kit_save_load_round_trip() {
        let mut kit_file = OpenOptions::new()
            .read(true)
            .open(DEFAULT_KIT_PATH)
            .unwrap();
        let kit = Kit::load(kit_file).expect("fail to load kit.default");
        let default_kit = Kit::default();
        assert_eq!(kit, default_kit, "kit default is not same as before");

        kit_file = OpenOptions::new()
            .write(true)
            .open(DEFAULT_KIT_PATH)
            .unwrap();
        default_kit.save(kit_file).expect("fail to export kit");

        kit_file = OpenOptions::new()
            .read(true)
            .open(DEFAULT_KIT_PATH)
            .unwrap();
        let kit = Kit::load(kit_file).expect("fail to load kit.default");
        assert_eq!(kit, default_kit, "kit roudtrip does not work");
    }

    #[cfg(feature = "fs")]
    #[test]
    fn armored_kit_save_load_round_trip() {
        let kit_file = OpenOptions::new()
            .read(true)
            .open(DEFAULT_KIT_PATH)
            .unwrap();
        let kit = Kit::load(kit_file).expect("fail to load kit.default");
        let unarmored_kit =
            Kit::load_armored(ARMORED_KIT_PATH).expect("fail to export armored kit");
        assert_eq!(kit, unarmored_kit, "kit unarmored is not the same");

        let default_kit = Kit::default();
        default_kit
            .save_armored(ARMORED_KIT_PATH)
            .expect("fail to save armored kit");
        let kit = Kit::load_armored(ARMORED_KIT_PATH).expect("fail to export armored kit");
        assert_eq!(kit, default_kit, "armored kit roudtrip does not work");
    }

    // A contract with almost default fields
    fn almost_default_contract() -> Contract {
        Contract {
            version: Default::default(),
            transfer: Default::default(),
            terminals: Default::default(),
            genesis: rgb::Genesis {
                ffv: Default::default(),
                schema_id: rgb::SchemaId::from_str(
                    "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA#distant-history-exotic",
                )
                .unwrap(),
                timestamp: Default::default(),
                issuer: Default::default(),
                chain_net: Default::default(),
                seal_closing_strategy: Default::default(),
                metadata: Default::default(),
                globals: Default::default(),
                assignments: Default::default(),
            },
            bundles: Default::default(),
            schema: rgb::Schema {
                ffv: Default::default(),
                name: strict_encoding::TypeName::from_str("Name").unwrap(),
                meta_types: Default::default(),
                global_types: Default::default(),
                owned_types: Default::default(),
                genesis: Default::default(),
                transitions: Default::default(),
                default_assignment: Default::default(),
            },
            types: Default::default(),
            scripts: Default::default(),
        }
    }

    #[test]
    fn contract_save_load_round_trip() {
        let mut contract_file = OpenOptions::new()
            .read(true)
            .open(DEFAULT_CONTRACT_PATH)
            .unwrap();
        let contract = Contract::load(contract_file).expect("fail to load contract.default");

        let default_contract = almost_default_contract();
        assert_eq!(&contract, &default_contract, "contract default is not same as before");

        contract_file = OpenOptions::new()
            .write(true)
            .open(DEFAULT_CONTRACT_PATH)
            .unwrap();
        default_contract
            .save(contract_file)
            .expect("fail to export contract");

        contract_file = OpenOptions::new()
            .read(true)
            .open(DEFAULT_CONTRACT_PATH)
            .unwrap();
        let contract = Contract::load(contract_file).expect("fail to load contract.default");
        assert_eq!(&contract, &default_contract, "contract roudtrip does not work");
    }

    #[cfg(feature = "fs")]
    #[test]
    fn armored_contract_save_load_round_trip() {
        let contract_file = OpenOptions::new()
            .read(true)
            .open(DEFAULT_CONTRACT_PATH)
            .unwrap();
        let contract = Contract::load(contract_file).expect("fail to load contract.default");
        let unarmored_contract =
            Contract::load_armored(ARMORED_CONTRACT_PATH).expect("fail to export armored contract");
        assert_eq!(contract, unarmored_contract, "contract unarmored is not the same");

        let default_contract = almost_default_contract();
        default_contract
            .save_armored(ARMORED_CONTRACT_PATH)
            .expect("fail to save armored contract");
        let contract =
            Contract::load_armored(ARMORED_CONTRACT_PATH).expect("fail to export armored contract");
        assert_eq!(contract, default_contract, "armored contract roudtrip does not work");
    }

    // A transfer with almost default fields
    fn almost_default_transfer() -> Transfer {
        Transfer {
            version: Default::default(),
            transfer: true,
            terminals: Default::default(),
            genesis: rgb::Genesis {
                ffv: Default::default(),
                schema_id: rgb::SchemaId::from_str(
                    "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA#distant-history-exotic",
                )
                .unwrap(),
                timestamp: Default::default(),
                issuer: Default::default(),
                chain_net: Default::default(),
                seal_closing_strategy: Default::default(),
                metadata: Default::default(),
                globals: Default::default(),
                assignments: Default::default(),
            },
            bundles: Default::default(),
            schema: rgb::Schema {
                ffv: Default::default(),
                name: strict_encoding::TypeName::from_str("Name").unwrap(),
                meta_types: Default::default(),
                global_types: Default::default(),
                owned_types: Default::default(),
                genesis: Default::default(),
                transitions: Default::default(),
                default_assignment: Default::default(),
            },
            types: Default::default(),
            scripts: Default::default(),
        }
    }

    #[cfg(feature = "fs")]
    #[test]
    fn transfer_save_load_round_trip() {
        let transfer =
            Transfer::load_file(DEFAULT_TRANSFER_PATH).expect("fail to load transfer.default");

        let default_transfer = almost_default_transfer();
        assert_eq!(&transfer, &default_transfer, "transfer default is not same as before");

        default_transfer
            .save_file(DEFAULT_TRANSFER_PATH)
            .expect("fail to export transfer");

        let transfer =
            Transfer::load_file(DEFAULT_TRANSFER_PATH).expect("fail to load transfer.default");
        assert_eq!(&transfer, &default_transfer, "transfer roudtrip does not work");
    }

    #[cfg(feature = "fs")]
    #[test]
    fn valid_transfer_save_load_round_trip() {
        let valid_transfer = ValidTransfer::load_file(DEFAULT_VALID_TRANSFER_PATH)
            .expect("fail to load valid transfer.default");

        let default_transfer = almost_default_transfer();
        let default_valid_transfer =
            ValidTransfer::from_parts(default_transfer, validation::Status::default());
        assert_eq!(
            valid_transfer.into_consignment(),
            default_valid_transfer.clone().into_consignment(),
            "valid transfer default is not same as before"
        );

        default_valid_transfer
            .save_file(DEFAULT_VALID_TRANSFER_PATH)
            .expect("fail to export transfer");

        let valid_transfer = ValidTransfer::load_file(DEFAULT_VALID_TRANSFER_PATH)
            .expect("fail to load valid transfer.default");
        assert_eq!(
            valid_transfer.into_consignment(),
            default_valid_transfer.into_consignment(),
            "valid transfer roudtrip does not work"
        );
    }

    #[cfg(feature = "fs")]
    #[test]
    fn armored_transfer_save_load_round_trip() {
        let transfer =
            Transfer::load_file(DEFAULT_TRANSFER_PATH).expect("fail to load transfer.default");
        let unarmored_transfer =
            Transfer::load_armored(ARMORED_TRANSFER_PATH).expect("fail to export armored transfer");
        assert_eq!(transfer, unarmored_transfer, "transfer unarmored is not the same");

        let default_transfer = almost_default_transfer();
        default_transfer
            .save_armored(ARMORED_TRANSFER_PATH)
            .expect("fail to save armored transfer");
        let transfer =
            Transfer::load_armored(ARMORED_TRANSFER_PATH).expect("fail to export armored transfer");
        assert_eq!(transfer, default_transfer, "armored transfer roudtrip does not work");
    }
}
