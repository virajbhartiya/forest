// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
//! This module contains the logic for storing and verifying the proofs parameters.
//!
//! The parameters are fetched from the network and stored in the cache directory. The cache directory can be set
//! using the [`PROOFS_PARAMETER_CACHE_ENV`] environment variable. If not set, the default directory is used.

use std::{
    fs::File as SyncFile,
    io::{self, copy as sync_copy, BufReader as SyncBufReader},
    path::{Path, PathBuf},
};

use ahash::HashMap;
use anyhow::{bail, Context};
use blake2b_simd::{Hash, State as Blake2b};
use cid::Cid;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use super::is_env_truthy;

const PROOF_DIGEST_LEN: usize = 16;

/// Environment variable that allows skipping checksum verification of the parameter files.
const FOREST_FORCE_TRUST_PARAMS_ENV: &str = "FOREST_FORCE_TRUST_PARAMS";

/// Environment variable to set the directory where proofs parameters are stored. Defaults to
/// [`PARAM_DIR`] in the data directory.
pub(super) const PROOFS_PARAMETER_CACHE_ENV: &str = "FIL_PROOFS_PARAMETER_CACHE";

/// Default directory name for storing proofs parameters.
const PARAM_DIR: &str = "filecoin-proof-parameters";

/// Default parameters, as outlined in Lotus `v1.26.2`.
/// <https://github.com/filecoin-project/filecoin-ffi/blob/b715c9403faf919e95fdc702cd651e842f18d890/parameters.json>
pub(super) const DEFAULT_PARAMETERS: &str = include_str!("./parameters.json");

/// Map of parameter data, to be deserialized from the parameter file.
pub(super) type ParameterMap = HashMap<String, ParameterData>;

/// Data structure for retrieving the proof parameter data from provided JSON.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub(super) struct ParameterData {
    #[serde(with = "crate::lotus_json::stringify")]
    pub cid: Cid,
    #[serde(with = "hex::serde")]
    pub digest: [u8; PROOF_DIGEST_LEN],
    pub sector_size: u64,
}

/// Ensures the parameter file is downloaded and has the correct checksum.
/// This behavior can be disabled by setting the [`FOREST_FORCE_TRUST_PARAMS_ENV`] environment variable to 1.
pub(super) async fn check_parameter_file(path: &Path, info: &ParameterData) -> anyhow::Result<()> {
    if is_env_truthy(FOREST_FORCE_TRUST_PARAMS_ENV) {
        warn!("Assuming parameter files are okay. Do not use in production!");
        return Ok(());
    }

    let hash = tokio::task::spawn_blocking({
        let file = SyncFile::open(path)?;
        move || -> Result<Hash, io::Error> {
            let mut reader = SyncBufReader::new(file);
            let mut hasher = Blake2b::new();
            sync_copy(&mut reader, &mut hasher)?;
            Ok(hasher.finalize())
        }
    })
    .await??;

    let hash_chunk = hash
        .as_bytes()
        .get(..PROOF_DIGEST_LEN)
        .context("invalid digest length")?;
    if info.digest == hash_chunk {
        debug!("Parameter file {:?} is ok", path);
        Ok(())
    } else {
        bail!(
            "Checksum mismatch in param file {:?}. ({:x?} != {:x?})",
            path,
            hash_chunk,
            info.digest,
        )
    }
}

// Proof parameter file directory. Defaults to
// %DATA_DIR/filecoin-proof-parameters unless the FIL_PROOFS_PARAMETER_CACHE
// environment variable is set.
pub(super) fn param_dir(data_dir: &Path) -> PathBuf {
    std::env::var(PathBuf::from(PROOFS_PARAMETER_CACHE_ENV))
        .map(PathBuf::from)
        .unwrap_or_else(|_| data_dir.join(PARAM_DIR))
}

/// Forest uses a set of external crates for verifying the proofs generated by
/// the miners. These external crates require a specific set of parameter files
/// to be located at in a specific folder. By default, it is
/// `/var/tmp/filecoin-proof-parameters` but it can be overridden by the
/// `FIL_PROOFS_PARAMETER_CACHE` environment variable. Forest will automatically
/// download the parameter files from Cloudflare/IPFS and verify their validity. For
/// consistency, Forest will prefer to download the files it's local data
/// directory. To this end, the `FIL_PROOFS_PARAMETER_CACHE` environment
/// variable is updated before the parameters are downloaded.
///
/// More information available [here](https://github.com/filecoin-project/rust-fil-proofs/blob/8f5bd86be36a55e33b9b293ba22ea13ca1f28163/README.md?plain=1#L219-L235).
pub fn set_proofs_parameter_cache_dir_env(data_dir: &Path) {
    std::env::set_var(PROOFS_PARAMETER_CACHE_ENV, param_dir(data_dir));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_proof_file_check() {
        let tempfile = tempfile::Builder::new().tempfile().unwrap();
        let path = tempfile.path();

        let data = b"Cthulhu fhtagn!";
        std::fs::write(path, data).unwrap();

        let mut hasher = Blake2b::new();
        hasher.update(data);
        let digest = hasher
            .finalize()
            .as_bytes()
            .get(..PROOF_DIGEST_LEN)
            .unwrap()
            .to_owned();

        let param_data = ParameterData {
            cid: Cid::default(),
            digest: digest.try_into().unwrap(),
            sector_size: 32,
        };

        check_parameter_file(path, &param_data).await.unwrap()
    }

    #[tokio::test]
    async fn test_proof_file_check_no_file() {
        let param_data = ParameterData {
            cid: Cid::default(),
            digest: [0; PROOF_DIGEST_LEN],
            sector_size: 32,
        };

        let path = Path::new("cthulhuazathoh.dagon");
        let ret = check_parameter_file(path, &param_data).await;
        assert!(
            ret.unwrap_err().downcast_ref::<io::Error>().unwrap().kind() == io::ErrorKind::NotFound
        );
    }
}
