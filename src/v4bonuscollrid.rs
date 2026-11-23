//! `v4bonuscollrid`: the v4bonuscurve scan with the Spoke bonus queried by collateral reserve id instead of debt reserve id. Reads only.

use std::path::PathBuf;

pub async fn v4bonuscollrid(network: String, start: u64, blocks: u64, commit: String, tolerance_bps: u64, seed: u64, out: PathBuf) -> anyhow::Result<()> {
    crate::v4bonuscurve::run("v4bonuscollrid", true, network, start, blocks, commit, tolerance_bps, seed, out).await
}
