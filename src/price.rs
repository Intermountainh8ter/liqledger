//! v2: USD valuation and per-feed attribution (pre-registration addendum A).
//!
//! Prices come only from Chainlink `AnswerUpdated` logs of the aggregator behind
//! each protocol oracle's price source, read at the record's block. Assets whose
//! source is not a Chainlink-style proxy are left unpriced and counted. Morpho
//! Blue records are not priced in this version.

use crate::{AnswerUpdated, LiquidationRecord};
use alloy::primitives::{Address, I256, U256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::Filter;
use alloy::sol;
use alloy::sol_types::SolEvent;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::io::Write as _;
use std::path::PathBuf;

sol! {
    #[sol(rpc)]
    interface IOracle {
        function getSourceOfAsset(address asset) external view returns (address);
    }
    #[sol(rpc)]
    interface IFeed {
        function aggregator() external view returns (address);
        function decimals() external view returns (uint8);
        function description() external view returns (string);
    }
    #[sol(rpc)]
    interface IErc20 {
        function decimals() external view returns (uint8);
    }
}

const AAVE_ORACLE: &str = "0x54586bE62E3c3580375aE3723C145253060Ca0C2";
const SPARK_ORACLE: &str = "0x8105f69D9C41644c6A0803fDA7D03Aa70996cFD9";
const WETH: &str = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
const CHUNK: u64 = 50_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedInfo {
    pub protocol: String,
    pub asset: String,
    pub source: String,
    pub aggregator: Option<String>,
    pub description: Option<String>,
    pub feed_decimals: Option<u8>,
    pub token_decimals: Option<u8>,
    pub n_answers_in_window: usize,
    pub unpriced_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricedRecord {
    #[serde(flatten)]
    pub rec: LiquidationRecord,
    pub priced: bool,
    pub unpriced_reason: Option<String>,
    pub collateral_px_usd: Option<f64>,
    pub debt_px_usd: Option<f64>,
    pub seized_usd: Option<f64>,
    pub repaid_usd: Option<f64>,
    pub gross_profit_usd: Option<f64>,
    pub eth_px_usd: Option<f64>,
    pub priority_fee_usd: Option<f64>,
    /// priority_fee_usd / gross_profit_usd; None when fee is zero or profit <= 0.
    pub visible_bid_share: Option<f64>,
    pub blocks_since_collateral_feed: Option<u64>,
    pub blocks_since_debt_feed: Option<u64>,
    pub own_feed_same_block: Option<bool>,
}

struct Answers {
    /// (block, answer) ascending by block.
    points: Vec<(u64, f64)>,
}

impl Answers {
    fn latest_at(&self, block: u64) -> Option<(u64, f64)> {
        let idx = self.points.partition_point(|(b, _)| *b <= block);
        if idx == 0 { None } else { Some(self.points[idx - 1]) }
    }
}

fn to_f64_scaled(v: &str, decimals: u8) -> Option<f64> {
    let u: U256 = v.parse().ok()?;
    // Exact enough for USD reporting: convert via string to avoid u128 overflow on 1e36-scale values.
    let s = u.to_string();
    let f: f64 = s.parse().ok()?;
    Some(f / 10f64.powi(decimals as i32))
}

async fn stream_answers<P: Provider>(p: &P, aggregator: Address, from: u64, to: u64, feed_decimals: u8) -> anyhow::Result<Answers> {
    let mut points = Vec::new();
    let mut start = from;
    while start <= to {
        let end = (start + CHUNK - 1).min(to);
        let f = Filter::new().address(aggregator).event_signature(AnswerUpdated::SIGNATURE_HASH).from_block(start).to_block(end);
        let logs = p.get_logs(&f).await?;
        for l in logs {
            let Some(bn) = l.block_number else { continue };
            // topics[1] = current (int256 indexed)
            let topics = l.topics();
            if topics.len() < 2 { continue; }
            let cur = I256::from_be_bytes(topics[1].0);
            let as_f: f64 = cur.to_string().parse().unwrap_or(f64::NAN);
            points.push((bn, as_f / 10f64.powi(feed_decimals as i32)));
        }
        start = end + 1;
    }
    points.sort_by_key(|(b, _)| *b);
    Ok(Answers { points })
}

pub async fn price(rpc: String, records: PathBuf, out: PathBuf, feeds_out: PathBuf) -> anyhow::Result<()> {
    let provider = ProviderBuilder::new().connect_http(rpc.parse()?);
    let text = std::fs::read_to_string(&records)?;
    let mut recs: Vec<LiquidationRecord> = Vec::new();
    for line in text.lines() {
        if !line.trim().is_empty() { recs.push(serde_json::from_str(line)?); }
    }
    anyhow::ensure!(!recs.is_empty(), "no records");
    let min_block = recs.iter().map(|r| r.block).min().unwrap();
    let max_block = recs.iter().map(|r| r.block).max().unwrap();
    let floor = 22_431_084u64.min(min_block);
    let window_from = min_block.saturating_sub(CHUNK).max(floor);

    // 1. distinct (protocol, asset) for Aave V3 and Spark.
    let mut keys: Vec<(String, String)> = Vec::new();
    for r in &recs {
        if r.protocol == "morpho-blue" { continue; }
        for a in [&r.collateral_asset, &r.debt_asset] {
            let k = (r.protocol.clone(), a.to_lowercase());
            if !keys.contains(&k) { keys.push(k); }
        }
    }
    eprintln!("{} distinct (protocol, asset) pairs to resolve", keys.len());

    // 2. resolve sources, aggregators, decimals.
    let mut feeds: BTreeMap<String, FeedInfo> = BTreeMap::new();
    let call_timeout = std::time::Duration::from_secs(20);
    for (i, (proto, asset)) in keys.iter().enumerate() {
        let oracle: Address = if proto == "aave-v3" { AAVE_ORACLE.parse()? } else { SPARK_ORACLE.parse()? };
        let asset_addr: Address = asset.parse()?;
        let mut info = FeedInfo { protocol: proto.clone(), asset: asset.clone(), source: String::new(), aggregator: None, description: None, feed_decimals: None, token_decimals: None, n_answers_in_window: 0, unpriced_reason: None };
        let source = match tokio::time::timeout(call_timeout, IOracle::new(oracle, &provider).getSourceOfAsset(asset_addr).call()).await {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => { info.unpriced_reason = Some(format!("getSourceOfAsset failed: {e}")); feeds.insert(format!("{proto}:{asset}"), info); continue; }
            Err(_) => { info.unpriced_reason = Some("getSourceOfAsset timed out".into()); feeds.insert(format!("{proto}:{asset}"), info); continue; }
        };
        info.source = format!("{source:#x}");
        let feed = IFeed::new(source, &provider);
        match tokio::time::timeout(call_timeout, feed.aggregator().call()).await {
            Ok(Ok(a)) => info.aggregator = Some(format!("{a:#x}")),
            Ok(Err(_)) => { info.unpriced_reason = Some("source has no aggregator(): custom adapter".into()); }
            Err(_) => { info.unpriced_reason = Some("aggregator() timed out".into()); }
        }
        if let Ok(Ok(d)) = tokio::time::timeout(call_timeout, feed.decimals().call()).await { info.feed_decimals = Some(d); }
        if let Ok(Ok(d)) = tokio::time::timeout(call_timeout, feed.description().call()).await { info.description = Some(d); }
        match tokio::time::timeout(call_timeout, IErc20::new(asset_addr, &provider).decimals().call()).await {
            Ok(Ok(d)) => info.token_decimals = Some(d),
            Ok(Err(e)) => { info.unpriced_reason.get_or_insert(format!("token decimals() failed: {e}")); }
            Err(_) => { info.unpriced_reason.get_or_insert("token decimals() timed out".into()); }
        }
        if info.aggregator.is_some() && info.feed_decimals.is_none() { info.unpriced_reason = Some("feed decimals() failed".into()); }
        eprintln!("[{}/{}] {proto} {asset} -> {} {}", i + 1, keys.len(), info.description.as_deref().unwrap_or("?"), info.unpriced_reason.as_deref().unwrap_or("ok"));
        feeds.insert(format!("{proto}:{asset}"), info);
    }

    // 3. stream answers per aggregator.
    let mut answers: HashMap<String, Answers> = HashMap::new();
    for info in feeds.values_mut() {
        let (Some(agg), Some(dec)) = (&info.aggregator, info.feed_decimals) else { continue };
        if answers.contains_key(agg) { info.n_answers_in_window = answers[agg].points.len(); continue; }
        let a = stream_answers(&provider, agg.parse()?, window_from, max_block, dec).await?;
        eprintln!("{} ({}): {} answers in window", info.description.as_deref().unwrap_or("?"), &agg[..10], a.points.len());
        info.n_answers_in_window = a.points.len();
        if a.points.is_empty() { info.unpriced_reason = Some("no AnswerUpdated logs in window (aggregator newer than the records?)".into()); }
        answers.insert(agg.clone(), a);
    }
    std::fs::write(&feeds_out, serde_json::to_string_pretty(&feeds)?)?;

    // ETH/USD from the Aave WETH feed.
    let eth_feed = feeds.get(&format!("aave-v3:{WETH}")).and_then(|f| f.aggregator.clone());

    // 4. price every record.
    let mut file = std::fs::File::create(&out)?;
    let (mut n_priced, mut n_unpriced) = (0usize, 0usize);
    let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
    for r in recs {
        let mut p = PricedRecord { rec: r.clone(), priced: false, unpriced_reason: None, collateral_px_usd: None, debt_px_usd: None, seized_usd: None, repaid_usd: None, gross_profit_usd: None, eth_px_usd: None, priority_fee_usd: None, visible_bid_share: None, blocks_since_collateral_feed: None, blocks_since_debt_feed: None, own_feed_same_block: None };
        if let Some(agg) = &eth_feed {
            if let Some((_, px)) = answers.get(agg).and_then(|a| a.latest_at(r.block)) {
                p.eth_px_usd = Some(px);
                let fee_eth: f64 = r.priority_fee_wei.parse::<f64>().unwrap_or(0.0) / 1e18;
                p.priority_fee_usd = Some(fee_eth * px);
            }
        }
        if r.protocol == "morpho-blue" {
            p.unpriced_reason = Some("morpho-blue: market oracle, not priced in v2".into());
        } else {
            let ck = format!("{}:{}", r.protocol, r.collateral_asset.to_lowercase());
            let dk = format!("{}:{}", r.protocol, r.debt_asset.to_lowercase());
            let (ci, di) = (feeds.get(&ck), feeds.get(&dk));
            let side = |info: Option<&FeedInfo>, amount: &str| -> Result<(f64, f64, u64, bool), String> {
                let info = info.ok_or("feed missing")?;
                if let Some(why) = &info.unpriced_reason { return Err(why.clone()); }
                let agg = info.aggregator.as_ref().ok_or("no aggregator")?;
                let (blk, px) = answers.get(agg).and_then(|a| a.latest_at(r.block)).ok_or("no answer at or before block")?;
                let amt = to_f64_scaled(amount, info.token_decimals.ok_or("no token decimals")?).ok_or("amount parse")?;
                Ok((px, amt * px, r.block - blk, blk == r.block))
            };
            match (side(ci, &r.collateral_seized), side(di, &r.debt_repaid)) {
                (Ok((cpx, seized, csince, csame)), Ok((dpx, repaid, dsince, dsame))) => {
                    p.priced = true;
                    p.collateral_px_usd = Some(cpx);
                    p.debt_px_usd = Some(dpx);
                    p.seized_usd = Some(seized);
                    p.repaid_usd = Some(repaid);
                    let gp = seized - repaid;
                    p.gross_profit_usd = Some(gp);
                    p.blocks_since_collateral_feed = Some(csince);
                    p.blocks_since_debt_feed = Some(dsince);
                    p.own_feed_same_block = Some(csame || dsame);
                    if let Some(fee) = p.priority_fee_usd {
                        if fee > 0.0 && gp > 0.0 { p.visible_bid_share = Some(fee / gp); }
                    }
                }
                (Err(e), _) | (_, Err(e)) => { p.unpriced_reason = Some(e); }
            }
        }
        if p.priced { n_priced += 1; } else { n_unpriced += 1; *reasons.entry(p.unpriced_reason.clone().unwrap_or_default()).or_insert(0) += 1; }
        writeln!(file, "{}", serde_json::to_string(&p)?)?;
    }
    eprintln!("priced {n_priced}, unpriced {n_unpriced}: {reasons:?}");
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct SummaryV2 {
    pub tool: String,
    pub n_records: usize,
    pub n_priced: usize,
    pub unpriced_reasons: BTreeMap<String, usize>,
    pub gross_profit_usd_total: f64,
    pub gross_profit_usd_by_protocol: BTreeMap<String, f64>,
    pub gross_profit_usd_median: f64,
    pub gross_profit_usd_p90: f64,
    /// H6: share of total gross profit carried by the top 10% of priced liquidations.
    pub top10pct_profit_share: crate::Ci,
    pub n_with_visible_fee: usize,
    /// H4: median visible bid share among records with a non-zero priority fee and positive profit.
    pub visible_bid_share_median: Option<crate::Ci>,
    pub visible_bid_share_by_size_decile: Vec<(u8, usize, f64)>,
    pub visible_bid_share_by_builder_top5: Vec<(String, usize, f64)>,
    /// H5: own collateral or debt feed updated in the same block.
    pub own_feed_same_block_share: crate::Ci,
    pub median_blocks_since_own_feed: Option<f64>,
    pub h4_visible_bids_small: String,
    pub h5_own_feed_adjacency: String,
    pub h6_bonus_concentration: String,
}

fn median(v: &mut [f64]) -> f64 {
    if v.is_empty() { return f64::NAN; }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    crate::percentile(v, 0.5)
}

pub fn summarize_v2(priced: PathBuf, out: PathBuf) -> anyhow::Result<()> {
    let text = std::fs::read_to_string(&priced)?;
    let mut all: Vec<PricedRecord> = Vec::new();
    for line in text.lines() { if !line.trim().is_empty() { all.push(serde_json::from_str(line)?); } }
    let priced_recs: Vec<&PricedRecord> = all.iter().filter(|p| p.priced).collect();
    let n = priced_recs.len();
    let mut reasons = BTreeMap::new();
    for p in all.iter().filter(|p| !p.priced) { *reasons.entry(p.unpriced_reason.clone().unwrap_or_default()).or_insert(0) += 1; }
    anyhow::ensure!(n > 0, "no priced records");

    let gp: Vec<f64> = priced_recs.iter().map(|p| p.gross_profit_usd.unwrap()).collect();
    let total: f64 = gp.iter().sum();
    let mut by_proto: BTreeMap<String, f64> = BTreeMap::new();
    for p in &priced_recs { *by_proto.entry(p.rec.protocol.clone()).or_insert(0.0) += p.gross_profit_usd.unwrap(); }
    let mut gp_sorted = gp.clone();
    gp_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

    // H6: top-10% share, bootstrapped.
    let top10 = crate::bootstrap(n, |idx| {
        let mut v: Vec<f64> = idx.iter().map(|&i| gp[i]).collect();
        v.sort_by(|a, b| b.partial_cmp(a).unwrap());
        let k = (v.len() / 10).max(1);
        let t: f64 = v.iter().sum();
        if t <= 0.0 { f64::NAN } else { v.iter().take(k).sum::<f64>() / t }
    }, 42);

    // H4: median visible bid share.
    let vis: Vec<f64> = priced_recs.iter().filter_map(|p| p.visible_bid_share).collect();
    let h4 = if vis.len() >= 100 {
        Some(crate::bootstrap(vis.len(), |idx| { let mut v: Vec<f64> = idx.iter().map(|&i| vis[i]).collect(); median(&mut v) }, 43))
    } else { None };

    // deciles by gross profit among records with visible fee.
    let mut with_fee: Vec<&PricedRecord> = priced_recs.iter().copied().filter(|p| p.visible_bid_share.is_some()).collect();
    with_fee.sort_by(|a, b| a.gross_profit_usd.partial_cmp(&b.gross_profit_usd).unwrap());
    let mut deciles = Vec::new();
    if !with_fee.is_empty() {
        let m = with_fee.len();
        for d in 0..10u8 {
            let lo = m * d as usize / 10; let hi = m * (d as usize + 1) / 10;
            if hi > lo { let mut v: Vec<f64> = with_fee[lo..hi].iter().map(|p| p.visible_bid_share.unwrap()).collect(); deciles.push((d + 1, hi - lo, median(&mut v))); }
        }
    }
    let mut by_builder: HashMap<String, Vec<f64>> = HashMap::new();
    for p in &with_fee { by_builder.entry(if p.rec.builder_extra_data.is_empty() { p.rec.builder.clone() } else { p.rec.builder_extra_data.clone() }).or_default().push(p.visible_bid_share.unwrap()); }
    let mut builders: Vec<(String, usize, f64)> = by_builder.into_iter().map(|(k, mut v)| (k, v.len(), median(&mut v))).collect();
    builders.sort_by(|a, b| b.1.cmp(&a.1));
    builders.truncate(5);

    // H5
    let same: Vec<bool> = priced_recs.iter().map(|p| p.own_feed_same_block.unwrap_or(false)).collect();
    let h5 = crate::bootstrap(n, crate::share_of(&same), 44);
    let mut since: Vec<f64> = priced_recs.iter().filter_map(|p| Some(p.blocks_since_collateral_feed?.min(p.blocks_since_debt_feed?) as f64)).collect();
    let med_since = if since.is_empty() { None } else { Some(median(&mut since)) };

    let verdict = |ok: bool, what: String| if ok { format!("supported: {what}") } else { format!("not supported: {what}") };
    let s = SummaryV2 {
        tool: format!("liqledger {}", env!("CARGO_PKG_VERSION")),
        n_records: all.len(), n_priced: n, unpriced_reasons: reasons,
        gross_profit_usd_total: total, gross_profit_usd_by_protocol: by_proto,
        gross_profit_usd_median: crate::percentile(&gp_sorted, 0.5), gross_profit_usd_p90: crate::percentile(&gp_sorted, 0.9),
        top10pct_profit_share: top10, n_with_visible_fee: vis.len(), visible_bid_share_median: h4,
        visible_bid_share_by_size_decile: deciles, visible_bid_share_by_builder_top5: builders,
        own_feed_same_block_share: h5, median_blocks_since_own_feed: med_since,
        h4_visible_bids_small: match h4 { Some(c) => verdict(c.hi < 0.50, format!("median visible bid share {:.3} [{:.3}, {:.3}] vs upper bound < 0.50 (n={})", c.point, c.lo, c.hi, vis.len())), None => format!("not evaluated: {} records with a visible fee < 100", vis.len()) },
        h5_own_feed_adjacency: verdict(h5.lo > 0.608, format!("own-feed same-block share {:.3} [{:.3}, {:.3}] vs lower bound > 0.608", h5.point, h5.lo, h5.hi)),
        h6_bonus_concentration: verdict(top10.lo > 0.80, format!("top-10% profit share {:.3} [{:.3}, {:.3}] vs lower bound > 0.80", top10.point, top10.lo, top10.hi)),
    };
    std::fs::write(&out, serde_json::to_string_pretty(&s)?)?;
    println!("{}", serde_json::to_string_pretty(&s)?);
    Ok(())
}
