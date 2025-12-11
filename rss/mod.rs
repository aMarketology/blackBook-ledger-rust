// ============================================================================
// RSS Module - Event Feed & Market Initialization
// ============================================================================
//
// This module handles RSS feed events from AI scrapers and converts them
// into on-chain prediction markets.
//
// Flow: RSS Feed → RssEvent → PendingEvent → Active Market (via CPMM)
// ============================================================================

pub mod market_rss;

pub use market_rss::*;
