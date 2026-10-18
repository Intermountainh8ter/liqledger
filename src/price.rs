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
        function phaseId() external view returns (uint16);
        function phaseAggregators(uint16 id) external view returns (address);
        function decimals() external view returns (uint8);
        function description() external view returns (string);
    }
    #[sol(rpc)]
    interface IErc20 {
        function decimals() external view returns (uint8);
    }
    /// Getters exposed by Aave's and Spark's adapter contracts (addendum B.1). Each kind answers a
    /// different subset; detection is "which of these do not revert at head".
    #[sol(rpc)]
    interface IAdapter {
        function latestAnswer() external view returns (int256);
        function ASSET_TO_USD_AGGREGATOR() external view returns (address);
        function getPriceCap() external view returns (int256);
        function BASE_TO_USD_AGGREGATOR() external view returns (address);
        function RATIO_PROVIDER() external view returns (address);
        function getRatio() external view returns (int256);
        function getSnapshotRatio() external view returns (uint256);
        function getSnapshotTimestamp() external view returns (uint256);
        function RATIO_DECIMALS() external view returns (uint8);
        function ASSET_TO_PEG() external view returns (address);
        function PEG_TO_BASE() external view returns (address);
        function chainlink() external view returns (address);
    }
}


/// Persisted reason strings must never carry raw transport error text: a keyed RPC URL can appear
/// in it, and feeds.json is committed to a public repository. Classify instead.
fn classify_call_error(what: &str, e: &impl std::fmt::Display) -> String {
    let msg = e.to_string().to_lowercase();
    if msg.contains("revert") || msg.contains("execution reverted") { format!("{what} reverted") } else { format!("{what} transport error") }
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
    /// Primary aggregator whose `AnswerUpdated` logs are streamed (plain proxy: itself; stable cap:
    /// the underlying; synchronicity: asset-to-peg; ratio cap: base; aggregate: the Chainlink component).
    pub aggregator: Option<String>,
    pub description: Option<String>,
    pub feed_decimals: Option<u8>,
    pub token_decimals: Option<u8>,
    pub n_answers_in_window: usize,
    pub unpriced_reason: Option<String>,
    /// Addendum B.1 kind: plain | stable-cap | synchronicity | ratio-cap | aggregate | fixed.
    #[serde(default)]
    pub kind: Option<String>,
    /// Second aggregator for synchronicity adapters (peg-to-base), with its decimals.
    #[serde(default)]
    pub aggregator2: Option<String>,
    #[serde(default)]
    pub feed2_decimals: Option<u8>,
    /// Stable cap (USD), read at head.
    #[serde(default)]
    pub price_cap: Option<f64>,
    /// Ratio-cap model: ratio(t) = snapshot_ratio + slope * (t - snapshot_ts).
    #[serde(default)]
    pub ratio_snapshot: Option<f64>,
    #[serde(default)]
    pub ratio_snapshot_ts: Option<u64>,
    #[serde(default)]
    pub ratio_head: Option<f64>,
    #[serde(default)]
    pub ratio_head_ts: Option<u64>,
    /// Fixed adapters: the constant USD price.
    #[serde(default)]
    pub fixed_price: Option<f64>,
    /// |composed head price / adapter latestAnswer at head - 1|; must be < 0.005.
    #[serde(default)]
    pub head_check_rel_err: Option<f64>,
    #[serde(default)]
    pub approx: bool,
    /// Addendum C.1: every phase aggregator of the primary proxy (current phase last).
    #[serde(default)]
    pub phase_aggregators: Vec<String>,
    #[serde(default)]
    pub phase_aggregators2: Vec<String>,
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
    #[serde(default)]
    pub collateral_source_kind: Option<String>,
    #[serde(default)]
    pub debt_source_kind: Option<String>,
    /// True when either side is a ratio-cap or multi-source aggregate (addendum B.2).
    #[serde(default)]
    pub approx: bool,
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
    let mut chunk = CHUNK;
    while start <= to {
        let end = (start + chunk - 1).min(to);
        let f = Filter::new().address(aggregator).event_signature(AnswerUpdated::SIGNATURE_HASH).from_block(start).to_block(end);
        // Busy feeds (ETH/USD updates every few blocks) can time out a 50k-block query on a full
        // node; halve the window and retry instead of failing the whole pass.
        let logs = match p.get_logs(&f).await {
            Ok(l) => l,
            Err(e) if chunk > 500 && e.to_string().contains("timed out") => { chunk /= 2; continue; }
            Err(e) => return Err(e.into()),
        };
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


fn ok<T, E1, E2>(r: Result<Result<T, E1>, E2>) -> Option<T> { r.ok().and_then(|x| x.ok()) }

async fn agg_of<P: Provider>(p: &P, proxy: Address, t: std::time::Duration) -> Result<(String, u8), String> {
    let f = IFeed::new(proxy, p);
    let a = tokio::time::timeout(t, f.aggregator().call()).await.map_err(|_| "aggregator() timed out".to_string())?.map_err(|e| classify_call_error("component aggregator()", &e))?;
    let d = tokio::time::timeout(t, f.decimals().call()).await.map_err(|_| "decimals() timed out".to_string())?.map_err(|e| classify_call_error("component decimals()", &e))?;
    Ok((format!("{a:#x}"), d))
}

/// Addendum C.1: all phase aggregators of a Chainlink proxy, oldest first, current phase last.
/// A proxy without `phaseId()` yields just its current aggregator.
async fn phases_of<P: Provider>(p: &P, proxy: Address, current: &str, t: std::time::Duration) -> Vec<String> {
    let f = IFeed::new(proxy, p);
    let mut out = Vec::new();
    if let Ok(Ok(n)) = tokio::time::timeout(t, f.phaseId().call()).await {
        for i in 1..=n {
            if let Ok(Ok(a)) = tokio::time::timeout(t, f.phaseAggregators(i).call()).await {
                let s = format!("{a:#x}");
                if a != Address::ZERO && s != current { out.push(s); }
            }
        }
    }
    out.push(current.to_string());
    out
}

fn i256_to_f64(v: alloy::primitives::I256, decimals: u8) -> f64 {
    v.to_string().parse::<f64>().unwrap_or(f64::NAN) / 10f64.powi(decimals as i32)
}

/// Addendum B.1: detect the adapter kind from which getters answer at head, fill the components,
/// and run the head consistency check. Returns the unpriced reason on failure.
async fn resolve_kind<P: Provider + Clone>(p: &P, source: Address, info: &mut FeedInfo, head_block: u64) -> Result<(), String> {
    let t = std::time::Duration::from_secs(20);
    let feed = IFeed::new(source, p);
    let ad = IAdapter::new(source, p);
    let src_decimals: u8 = ok(tokio::time::timeout(t, feed.decimals().call()).await).unwrap_or(8);
    info.feed_decimals = Some(src_decimals);
    let head_answer = ok(tokio::time::timeout(t, ad.latestAnswer().call()).await).map(|a| i256_to_f64(a, src_decimals));

    // plain proxy
    if let Some(a) = ok(tokio::time::timeout(t, feed.aggregator().call()).await) {
        info.kind = Some("plain".into());
        info.aggregator = Some(format!("{a:#x}"));
        info.phase_aggregators = phases_of(p, source, &format!("{a:#x}"), t).await;
        info.head_check_rel_err = Some(0.0);
        return Ok(());
    }
    // stable cap
    if let Some(under) = ok(tokio::time::timeout(t, ad.ASSET_TO_USD_AGGREGATOR().call()).await) {
        let (agg, dec) = agg_of(p, under, t).await?;
        let cap = ok(tokio::time::timeout(t, ad.getPriceCap().call()).await).ok_or("getPriceCap() failed")?;
        info.kind = Some("stable-cap".into());
        info.phase_aggregators = phases_of(p, under, &agg, t).await;
        info.aggregator = Some(agg); info.feed_decimals = Some(dec);
        info.price_cap = Some(i256_to_f64(cap, src_decimals));
        // head check: min(underlying latestAnswer, cap) vs adapter latestAnswer
        let u = ok(tokio::time::timeout(t, IAdapter::new(under, p).latestAnswer().call()).await).map(|a| i256_to_f64(a, dec)).ok_or("underlying latestAnswer() failed")?;
        return head_check(info, u.min(info.price_cap.unwrap()), head_answer);
    }
    // ratio cap
    if let Some(base) = ok(tokio::time::timeout(t, ad.BASE_TO_USD_AGGREGATOR().call()).await) {
        let (agg, dec) = agg_of(p, base, t).await?;
        let rd = ok(tokio::time::timeout(t, ad.RATIO_DECIMALS().call()).await).ok_or("RATIO_DECIMALS() failed")?;
        let ratio_head = ok(tokio::time::timeout(t, ad.getRatio().call()).await).map(|r| i256_to_f64(r, rd)).ok_or("getRatio() failed")?;
        let snap = ok(tokio::time::timeout(t, ad.getSnapshotRatio().call()).await).ok_or("getSnapshotRatio() failed")?;
        let snap_ts = ok(tokio::time::timeout(t, ad.getSnapshotTimestamp().call()).await).ok_or("getSnapshotTimestamp() failed")?;
        let head_ts = p.get_block_by_number(head_block.into()).await.ok().flatten().map(|b| b.header.timestamp).ok_or("head block timestamp")?;
        info.kind = Some("ratio-cap".into()); info.approx = true;
        info.phase_aggregators = phases_of(p, base, &agg, t).await;
        info.aggregator = Some(agg); info.feed_decimals = Some(dec);
        info.ratio_snapshot = Some(snap.to_string().parse::<f64>().unwrap_or(f64::NAN) / 10f64.powi(rd as i32));
        info.ratio_snapshot_ts = Some(snap_ts.to_string().parse().unwrap_or(0));
        info.ratio_head = Some(ratio_head); info.ratio_head_ts = Some(head_ts);
        let b = ok(tokio::time::timeout(t, IAdapter::new(base, p).latestAnswer().call()).await).map(|a| i256_to_f64(a, dec)).ok_or("base latestAnswer() failed")?;
        return head_check(info, b * ratio_head, head_answer);
    }
    // synchronicity (asset-to-peg × peg-to-base)
    if let Some(a2p) = ok(tokio::time::timeout(t, ad.ASSET_TO_PEG().call()).await) {
        let p2b = ok(tokio::time::timeout(t, ad.PEG_TO_BASE().call()).await).ok_or("PEG_TO_BASE() failed")?;
        let (agg1, dec1) = agg_of(p, a2p, t).await?;
        let (agg2, dec2) = agg_of(p, p2b, t).await?;
        info.kind = Some("synchronicity".into());
        info.phase_aggregators = phases_of(p, a2p, &agg1, t).await;
        info.phase_aggregators2 = phases_of(p, p2b, &agg2, t).await;
        info.aggregator = Some(agg1); info.feed_decimals = Some(dec1);
        info.aggregator2 = Some(agg2); info.feed2_decimals = Some(dec2);
        let x = ok(tokio::time::timeout(t, IAdapter::new(a2p, p).latestAnswer().call()).await).map(|a| i256_to_f64(a, dec1)).ok_or("asset-to-peg latestAnswer() failed")?;
        let y = ok(tokio::time::timeout(t, IAdapter::new(p2b, p).latestAnswer().call()).await).map(|a| i256_to_f64(a, dec2)).ok_or("peg-to-base latestAnswer() failed")?;
        return head_check(info, x * y, head_answer);
    }
    // multi-source aggregate: Chainlink component only
    if let Some(cl) = ok(tokio::time::timeout(t, ad.chainlink().call()).await) {
        let (agg, dec) = agg_of(p, cl, t).await?;
        info.kind = Some("aggregate".into()); info.approx = true;
        info.phase_aggregators = phases_of(p, cl, &agg, t).await;
        info.aggregator = Some(agg); info.feed_decimals = Some(dec);
        let c = ok(tokio::time::timeout(t, IAdapter::new(cl, p).latestAnswer().call()).await).map(|a| i256_to_f64(a, dec)).ok_or("chainlink component latestAnswer() failed")?;
        return head_check(info, c, head_answer);
    }
    // fixed: latestAnswer identical at head and head-100
    if let Some(h) = head_answer {
        let earlier = ok(tokio::time::timeout(t, ad.latestAnswer().block(alloy::eips::BlockId::number(head_block.saturating_sub(100))).call()).await).map(|a| i256_to_f64(a, src_decimals));
        if earlier == Some(h) && h.is_finite() && h > 0.0 {
            // Addendum C.2: constant alone is not enough; the source must say it is fixed, or be one of
            // Spark's undescribed constant feeds.
            let described_fixed = match info.description.as_deref() { None => true, Some(d) => d.starts_with("Fixed") || d == "ONE USD" };
            if !described_fixed { return Err("constant over 100 blocks but described as a live price".into()); }
            info.kind = Some("fixed".into());
            info.fixed_price = Some(h);
            info.head_check_rel_err = Some(0.0);
            return Ok(());
        }
        return Err("adapter exposes no known components and is not constant".into());
    }
    Err("source has no aggregator() and no latestAnswer()".into())
}

fn head_check(info: &mut FeedInfo, composed: f64, adapter: Option<f64>) -> Result<(), String> {
    let a = adapter.ok_or("adapter latestAnswer() failed")?;
    if !(a.is_finite() && a > 0.0) { return Err("adapter latestAnswer() not positive".into()); }
    let rel = (composed / a - 1.0).abs();
    info.head_check_rel_err = Some(rel);
    if rel < 0.005 { Ok(()) } else { Err(format!("head check failed: composed price off by {:.2}%", rel * 100.0)) }
}

/// Price at block `block` (timestamp `ts`) through the adapter rule for `info.kind`.
/// Returns (price, blocks since own feed [None for fixed], own feed updated in this block).
fn price_at(info: &FeedInfo, answers: &HashMap<String, Answers>, block: u64, ts: u64) -> Result<(f64, Option<u64>, bool), String> {
    let latest = |agg: &Option<String>| -> Result<(u64, f64), String> {
        let a = agg.as_ref().ok_or("no aggregator")?;
        answers.get(a).and_then(|x| x.latest_at(block)).ok_or_else(|| "no answer at or before block".to_string())
    };
    match info.kind.as_deref() {
        Some("plain") => { let (b, px) = latest(&info.aggregator)?; Ok((px, Some(block - b), b == block)) }
        Some("stable-cap") => { let (b, px) = latest(&info.aggregator)?; Ok((px.min(info.price_cap.unwrap_or(f64::INFINITY)), Some(block - b), b == block)) }
        Some("synchronicity") => {
            let (b1, x) = latest(&info.aggregator)?; let (b2, y) = latest(&info.aggregator2)?;
            let b = b1.max(b2);
            Ok((x * y, Some(block - b), b == block))
        }
        Some("ratio-cap") => {
            let (b, base) = latest(&info.aggregator)?;
            let (r0, t0, r1, t1) = (info.ratio_snapshot.ok_or("ratio model")?, info.ratio_snapshot_ts.ok_or("ratio model")? as f64, info.ratio_head.ok_or("ratio model")?, info.ratio_head_ts.ok_or("ratio model")? as f64);
            let slope = if t1 > t0 { (r1 - r0) / (t1 - t0) } else { 0.0 };
            let ratio = r0 + slope * (ts as f64 - t0);
            Ok((base * ratio, Some(block - b), b == block))
        }
        Some("aggregate") => { let (b, px) = latest(&info.aggregator)?; Ok((px, Some(block - b), b == block)) }
        Some("fixed") => Ok((info.fixed_price.ok_or("fixed price")?, None, false)),
        _ => Err(info.unpriced_reason.clone().unwrap_or_else(|| "unknown source kind".into())),
    }
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
    let head_block = provider.get_block_number().await?;
    let mut feeds: BTreeMap<String, FeedInfo> = BTreeMap::new();
    let call_timeout = std::time::Duration::from_secs(20);
    for (i, (proto, asset)) in keys.iter().enumerate() {
        let oracle: Address = if proto == "aave-v3" { AAVE_ORACLE.parse()? } else { SPARK_ORACLE.parse()? };
        let asset_addr: Address = asset.parse()?;
        let mut info = FeedInfo { protocol: proto.clone(), asset: asset.clone(), source: String::new(), aggregator: None, description: None, feed_decimals: None, token_decimals: None, n_answers_in_window: 0, unpriced_reason: None, kind: None, aggregator2: None, feed2_decimals: None, price_cap: None, ratio_snapshot: None, ratio_snapshot_ts: None, ratio_head: None, ratio_head_ts: None, fixed_price: None, head_check_rel_err: None, approx: false, phase_aggregators: vec![], phase_aggregators2: vec![] };
        let source = match tokio::time::timeout(call_timeout, IOracle::new(oracle, &provider).getSourceOfAsset(asset_addr).call()).await {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => { info.unpriced_reason = Some(classify_call_error("getSourceOfAsset", &e)); feeds.insert(format!("{proto}:{asset}"), info); continue; }
            Err(_) => { info.unpriced_reason = Some("getSourceOfAsset timed out".into()); feeds.insert(format!("{proto}:{asset}"), info); continue; }
        };
        info.source = format!("{source:#x}");
        let feed = IFeed::new(source, &provider);
        if let Ok(Ok(d)) = tokio::time::timeout(call_timeout, feed.description().call()).await { info.description = Some(d); }
        if let Err(why) = resolve_kind(&provider, source, &mut info, head_block).await {
            info.unpriced_reason = Some(why);
        }
        match tokio::time::timeout(call_timeout, IErc20::new(asset_addr, &provider).decimals().call()).await {
            Ok(Ok(d)) => info.token_decimals = Some(d),
            Ok(Err(e)) => { info.unpriced_reason.get_or_insert(classify_call_error("token decimals()", &e)); }
            Err(_) => { info.unpriced_reason.get_or_insert("token decimals() timed out".into()); }
        }
        if info.aggregator.is_some() && info.feed_decimals.is_none() { info.unpriced_reason = Some("feed decimals() failed".into()); }
        eprintln!("[{}/{}] {proto} {asset} -> {} {}", i + 1, keys.len(), info.description.as_deref().unwrap_or("?"), info.unpriced_reason.as_deref().unwrap_or("ok"));
        feeds.insert(format!("{proto}:{asset}"), info);
    }

    // 3. stream answers per aggregator.
    let mut answers: HashMap<String, Answers> = HashMap::new();
    for info in feeds.values_mut() {
        if info.unpriced_reason.is_some() { continue; }
        let pairs: Vec<(String, u8, Vec<String>)> = [(info.aggregator.clone(), info.feed_decimals, info.phase_aggregators.clone()), (info.aggregator2.clone(), info.feed2_decimals, info.phase_aggregators2.clone())]
            .into_iter().filter_map(|(a, d, ph)| Some((a?, d?, ph))).collect();
        for (agg, dec, phases) in pairs {
            if !answers.contains_key(&agg) {
                // Addendum C.1: the current aggregator's key holds the merged history of every phase.
                let mut merged: Vec<(u64, f64)> = Vec::new();
                let list = if phases.is_empty() { vec![agg.clone()] } else { phases.clone() };
                for ph in &list {
                    let a = stream_answers(&provider, ph.parse()?, window_from, max_block, dec).await?;
                    if list.len() > 1 { eprintln!("  phase {} : {} answers", &ph[..10], a.points.len()); }
                    merged.extend(a.points);
                }
                merged.sort_by_key(|(b, _)| *b);
                eprintln!("{} ({}): {} answers in window over {} phase(s)", info.description.as_deref().unwrap_or("?"), &agg[..10], merged.len(), list.len());
                answers.insert(agg.clone(), Answers { points: merged });
            }
            if Some(&agg) == info.aggregator.as_ref() { info.n_answers_in_window = answers[&agg].points.len(); }
            if answers[&agg].points.is_empty() { info.unpriced_reason = Some("no AnswerUpdated logs in window (aggregator newer than the records?)".into()); }
        }
    }
    std::fs::write(&feeds_out, serde_json::to_string_pretty(&feeds)?)?;

    // ETH/USD from the Aave WETH feed.
    let eth_feed = feeds.get(&format!("aave-v3:{WETH}")).and_then(|f| f.aggregator.clone());

    // 4. price every record.
    let mut file = std::fs::File::create(&out)?;
    let (mut n_priced, mut n_unpriced) = (0usize, 0usize);
    let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
    for r in recs {
        let mut p = PricedRecord { rec: r.clone(), priced: false, unpriced_reason: None, collateral_px_usd: None, debt_px_usd: None, seized_usd: None, repaid_usd: None, gross_profit_usd: None, eth_px_usd: None, priority_fee_usd: None, visible_bid_share: None, blocks_since_collateral_feed: None, blocks_since_debt_feed: None, own_feed_same_block: None, collateral_source_kind: None, debt_source_kind: None, approx: false };
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
            // Returns (price, usd value, blocks since own feed [None for fixed], same-block [false for fixed]).
            let side = |info: Option<&FeedInfo>, amount: &str| -> Result<(f64, f64, Option<u64>, bool), String> {
                let info = info.ok_or("feed missing")?;
                if let Some(why) = &info.unpriced_reason { return Err(why.clone()); }
                let amt = to_f64_scaled(amount, info.token_decimals.ok_or("no token decimals")?).ok_or("amount parse")?;
                let (px, since, same) = price_at(info, &answers, r.block, r.timestamp)?;
                Ok((px, amt * px, since, same))
            };
            p.collateral_source_kind = ci.and_then(|i| i.kind.clone());
            p.debt_source_kind = di.and_then(|i| i.kind.clone());
            match (side(ci, &r.collateral_seized), side(di, &r.debt_repaid)) {
                (Ok((cpx, seized, csince, csame)), Ok((dpx, repaid, dsince, dsame))) => {
                    p.priced = true;
                    p.approx = ci.map(|i| i.approx).unwrap_or(false) || di.map(|i| i.approx).unwrap_or(false);
                    p.collateral_px_usd = Some(cpx);
                    p.debt_px_usd = Some(dpx);
                    p.seized_usd = Some(seized);
                    p.repaid_usd = Some(repaid);
                    let gp = seized - repaid;
                    p.gross_profit_usd = Some(gp);
                    p.blocks_since_collateral_feed = csince;
                    p.blocks_since_debt_feed = dsince;
                    // Both sides fixed: no own feed exists; excluded from H5 (addendum B.2).
                    p.own_feed_same_block = if csince.is_none() && dsince.is_none() { None } else { Some(csame || dsame) };
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
    /// Addendum B: counts by exactness and by source kind pair; sensitivity on exact-priced records only.
    #[serde(default)]
    pub n_priced_exact: usize,
    #[serde(default)]
    pub n_priced_approx: usize,
    #[serde(default)]
    pub priced_by_kind_pair: BTreeMap<String, usize>,
    #[serde(default)]
    pub exact_only_visible_bid_share_median: Option<crate::Ci>,
    #[serde(default)]
    pub exact_only_own_feed_same_block_share: Option<crate::Ci>,
    #[serde(default)]
    pub exact_only_top10pct_profit_share: Option<crate::Ci>,
    #[serde(default)]
    pub n_in_h5: usize,
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

    // H5 over records that have an own feed at all (both-sides-fixed records carry None, addendum B.2).
    let same: Vec<bool> = priced_recs.iter().filter_map(|p| p.own_feed_same_block).collect();
    let h5 = crate::bootstrap(same.len().max(1), crate::share_of(&same), 44);

    // Addendum B: exactness counts, kind pairs, exact-only sensitivity.
    let exact: Vec<&PricedRecord> = priced_recs.iter().copied().filter(|p| !p.approx).collect();
    let n_exact = exact.len();
    let mut kind_pairs: BTreeMap<String, usize> = BTreeMap::new();
    for p in &priced_recs { *kind_pairs.entry(format!("{}/{}", p.collateral_source_kind.as_deref().unwrap_or("?"), p.debt_source_kind.as_deref().unwrap_or("?"))).or_insert(0) += 1; }
    let (ex_h4, ex_h5, ex_h6) = if n_exact >= 100 {
        let gpe: Vec<f64> = exact.iter().map(|p| p.gross_profit_usd.unwrap()).collect();
        let vise: Vec<f64> = exact.iter().filter_map(|p| p.visible_bid_share).collect();
        let samee: Vec<bool> = exact.iter().filter_map(|p| p.own_feed_same_block).collect();
        (
            if vise.len() >= 100 { Some(crate::bootstrap(vise.len(), |idx| { let mut v: Vec<f64> = idx.iter().map(|&i| vise[i]).collect(); median(&mut v) }, 45)) } else { None },
            Some(crate::bootstrap(samee.len().max(1), crate::share_of(&samee), 46)),
            Some(crate::bootstrap(n_exact, |idx| { let mut v: Vec<f64> = idx.iter().map(|&i| gpe[i]).collect(); v.sort_by(|a, b| b.total_cmp(a)); let k = (v.len() / 10).max(1); let t: f64 = v.iter().sum(); if t <= 0.0 { f64::NAN } else { v.iter().take(k).sum::<f64>() / t } }, 47)),
        )
    } else { (None, None, None) };
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
        h5_own_feed_adjacency: verdict(h5.lo > 0.608, format!("own-feed same-block share {:.3} [{:.3}, {:.3}] vs lower bound > 0.608 (n={})", h5.point, h5.lo, h5.hi, same.len())),
        h6_bonus_concentration: verdict(top10.lo > 0.80, format!("top-10% profit share {:.3} [{:.3}, {:.3}] vs lower bound > 0.80", top10.point, top10.lo, top10.hi)),
        n_priced_exact: n_exact, n_priced_approx: n - n_exact, priced_by_kind_pair: kind_pairs,
        exact_only_visible_bid_share_median: ex_h4, exact_only_own_feed_same_block_share: ex_h5, exact_only_top10pct_profit_share: ex_h6,
        n_in_h5: same.len(),
    };
    std::fs::write(&out, serde_json::to_string_pretty(&s)?)?;
    println!("{}", serde_json::to_string_pretty(&s)?);
    Ok(())
}
