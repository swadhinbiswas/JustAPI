use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Result of a single HTTP request.
struct RequestResult {
    latency: Duration,
    status: u16,
}

/// Aggregated profiling report.
pub struct ProfileReport {
    pub total_requests: u64,
    pub duration: Duration,
    pub latencies: Vec<Duration>,
    pub status_counts: HashMap<u16, u64>,
}

/// Unicode block characters for histogram rendering, from thinnest to full block.
const BLOCKS: [char; 8] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'];

/// Run the profiler against `addr` with the given concurrency and duration.
pub async fn run_profile(
    addr: &str,
    duration_secs: u64,
    connections: u64,
) -> anyhow::Result<ProfileReport> {
    let deadline = Duration::from_secs(duration_secs);

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<RequestResult>();

    for _ in 0..connections {
        let addr = addr.to_string();
        let tx = tx.clone();
        tokio::spawn(async move {
            let start = Instant::now();
            while start.elapsed() < deadline {
                if let Some(result) = send_request(&addr).await {
                    if tx.send(result).is_err() {
                        break;
                    }
                }
            }
        });
    }
    // Drop the original sender so `rx` will close once all tasks finish.
    drop(tx);

    let mut latencies = Vec::new();
    let mut status_counts: HashMap<u16, u64> = HashMap::new();

    while let Some(result) = rx.recv().await {
        latencies.push(result.latency);
        *status_counts.entry(result.status).or_insert(0) += 1;
    }

    Ok(ProfileReport {
        total_requests: latencies.len() as u64,
        duration: deadline,
        latencies,
        status_counts,
    })
}

/// Send a single raw HTTP/1.1 GET / request and return timing + status code.
async fn send_request(addr: &str) -> Option<RequestResult> {
    let start = Instant::now();
    let mut stream = TcpStream::connect(addr).await.ok()?;

    let request = format!(
        "GET / HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        addr
    );
    stream.write_all(request.as_bytes()).await.ok()?;

    let mut buf = vec![0u8; 4096];
    let n = stream.read(&mut buf).await.ok()?;
    let latency = start.elapsed();

    if n == 0 {
        return None;
    }

    let response = String::from_utf8_lossy(&buf[..n]);
    let status = parse_status_code(&response).unwrap_or(0);

    Some(RequestResult { latency, status })
}

/// Parse the HTTP status code from the first line of the response (e.g. "HTTP/1.1 200 OK").
fn parse_status_code(response: &str) -> Option<u16> {
    let first_line = response.lines().next()?;
    let mut parts = first_line.split_whitespace();
    parts.next()?; // skip "HTTP/1.1"
    parts.next()?.parse().ok()
}

/// Compute the value at the given percentile from a **sorted** slice of durations.
///
/// Uses the nearest-rank method: the index is `ceil(p/100 * N) - 1`, clamped to
/// valid bounds. Returns `Duration::ZERO` for an empty slice.
pub fn percentile(sorted: &[Duration], p: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    let rank = (p / 100.0 * sorted.len() as f64).ceil() as usize;
    let idx = rank.saturating_sub(1).min(sorted.len() - 1);
    sorted[idx]
}

/// Render a text-based latency histogram.
///
/// Divides the latency range into `bucket_count` equal-width buckets and draws
/// a horizontal bar for each bucket using Unicode block characters.
///
/// The bars are scaled so that the bucket with the most samples fills
/// `max_bar_width` character cells.
pub fn render_histogram(
    latencies: &[Duration],
    bucket_count: usize,
    max_bar_width: usize,
) -> String {
    if latencies.is_empty() || bucket_count == 0 {
        return String::from("  (no data)\n");
    }

    let min_us = latencies.iter().map(|d| d.as_micros()).min().unwrap_or(0);
    let max_us = latencies.iter().map(|d| d.as_micros()).max().unwrap_or(0);

    // Avoid division by zero when all latencies are identical.
    let range = if max_us == min_us { 1 } else { max_us - min_us };
    let bucket_width = (range as f64 / bucket_count as f64).ceil() as u128;
    let bucket_width = bucket_width.max(1);

    let mut buckets = vec![0u64; bucket_count];
    for d in latencies {
        let us = d.as_micros();
        let idx = ((us - min_us) / bucket_width) as usize;
        let idx = idx.min(bucket_count - 1);
        buckets[idx] += 1;
    }

    let max_count = *buckets.iter().max().unwrap_or(&1);
    let max_count = max_count.max(1);

    let mut out = String::new();
    for (i, &count) in buckets.iter().enumerate() {
        let lo = min_us + (i as u128) * bucket_width;
        let hi = lo + bucket_width;
        let bar = build_bar(count, max_count, max_bar_width);
        out.push_str(&format!(
            "  {:>8.1}ms - {:>8.1}ms │{} ({count})\n",
            lo as f64 / 1000.0,
            hi as f64 / 1000.0,
            bar,
        ));
    }
    out
}

/// Build a single bar string of the histogram.
///
/// Uses full-block characters for the integer part and a fractional sub-block
/// for the remainder.
fn build_bar(count: u64, max_count: u64, max_width: usize) -> String {
    let fraction = count as f64 / max_count as f64;
    let total_eighths = (fraction * max_width as f64 * 8.0).round() as usize;
    let full_blocks = total_eighths / 8;
    let remainder = total_eighths % 8;

    let mut bar = String::with_capacity(full_blocks + 1);
    for _ in 0..full_blocks {
        bar.push(BLOCKS[7]);
    }
    if remainder > 0 {
        bar.push(BLOCKS[remainder - 1]);
    }
    bar
}

/// Format the profile report as a human-readable string.
pub fn format_report(report: &ProfileReport) -> String {
    let mut out = String::new();
    out.push_str("╔══════════════════════════════════════════════════╗\n");
    out.push_str("║           JustAPI Profile Report                ║\n");
    out.push_str("╚══════════════════════════════════════════════════╝\n\n");

    let rps = if report.duration.as_secs_f64() > 0.0 {
        report.total_requests as f64 / report.duration.as_secs_f64()
    } else {
        0.0
    };

    out.push_str(&format!(
        "  Duration:        {:.1}s\n",
        report.duration.as_secs_f64()
    ));
    out.push_str(&format!("  Total requests:  {}\n", report.total_requests));
    out.push_str(&format!("  Requests/sec:    {rps:.1}\n\n"));

    if !report.latencies.is_empty() {
        let mut sorted = report.latencies.clone();
        sorted.sort();

        let p50 = percentile(&sorted, 50.0);
        let p95 = percentile(&sorted, 95.0);
        let p99 = percentile(&sorted, 99.0);

        out.push_str("  Latency percentiles:\n");
        out.push_str(&format!("    p50:  {:.3}ms\n", p50.as_secs_f64() * 1000.0));
        out.push_str(&format!("    p95:  {:.3}ms\n", p95.as_secs_f64() * 1000.0));
        out.push_str(&format!(
            "    p99:  {:.3}ms\n\n",
            p99.as_secs_f64() * 1000.0
        ));

        out.push_str("  Latency histogram:\n");
        out.push_str(&render_histogram(&sorted, 10, 30));
        out.push('\n');
    } else {
        out.push_str("  (no successful requests)\n\n");
    }

    out.push_str("  Status code distribution:\n");
    let mut codes: Vec<_> = report.status_counts.iter().collect();
    codes.sort_by_key(|(code, _)| *code);
    for (&code, &count) in &codes {
        out.push_str(&format!("    HTTP {code}: {count}\n"));
    }

    out
}

/// Write the report to a file.
pub fn save_report(report_text: &str, path: &Path) -> anyhow::Result<()> {
    let mut f = std::fs::File::create(path)?;
    f.write_all(report_text.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_percentile_basic() {
        // 10 sorted values: 1ms, 2ms, ... 10ms
        let latencies: Vec<Duration> = (1..=10).map(Duration::from_millis).collect();

        let p50 = percentile(&latencies, 50.0);
        assert_eq!(p50, Duration::from_millis(5));

        let p95 = percentile(&latencies, 95.0);
        // ceil(0.95 * 10) = 10, idx = 9 → 10ms
        assert_eq!(p95, Duration::from_millis(10));

        let p99 = percentile(&latencies, 99.0);
        assert_eq!(p99, Duration::from_millis(10));

        // Edge: p0 should give the first element
        let p0 = percentile(&latencies, 0.0);
        assert_eq!(p0, Duration::from_millis(1));

        // Edge: p100 should give the last element
        let p100 = percentile(&latencies, 100.0);
        assert_eq!(p100, Duration::from_millis(10));
    }

    #[test]
    fn test_percentile_empty() {
        let empty: Vec<Duration> = Vec::new();
        assert_eq!(percentile(&empty, 50.0), Duration::ZERO);
    }

    #[test]
    fn test_percentile_single() {
        let single = vec![Duration::from_millis(42)];
        assert_eq!(percentile(&single, 0.0), Duration::from_millis(42));
        assert_eq!(percentile(&single, 50.0), Duration::from_millis(42));
        assert_eq!(percentile(&single, 100.0), Duration::from_millis(42));
    }

    #[test]
    fn test_histogram_renders_nonempty() {
        let latencies: Vec<Duration> = (1..=100).map(Duration::from_millis).collect();
        let hist = render_histogram(&latencies, 5, 20);

        // Should contain multiple lines
        let lines: Vec<&str> = hist.lines().collect();
        assert_eq!(lines.len(), 5, "expected 5 histogram buckets");

        // Each line should contain the box-drawing separator
        for line in &lines {
            assert!(line.contains('│'), "each line should contain │ separator");
        }

        // Each line should have a count in parentheses
        for line in &lines {
            assert!(line.contains('('), "each line should contain count");
        }
    }

    #[test]
    fn test_histogram_empty() {
        let hist = render_histogram(&[], 5, 20);
        assert_eq!(hist, "  (no data)\n");
    }

    #[test]
    fn test_histogram_all_same() {
        // All latencies identical — should not panic from division by zero
        let latencies = vec![Duration::from_millis(5); 50];
        let hist = render_histogram(&latencies, 5, 20);
        assert!(!hist.is_empty());
        // The first bucket should have all 50 samples
        assert!(hist.contains("(50)"));
    }

    #[test]
    fn test_build_bar() {
        // Full bar
        let bar = build_bar(100, 100, 10);
        assert_eq!(bar.chars().count(), 10);

        // Empty bar
        let bar = build_bar(0, 100, 10);
        assert!(bar.is_empty());

        // Half bar
        let bar = build_bar(50, 100, 10);
        // 50/100 * 10 * 8 = 40 eighths → 5 full blocks
        assert_eq!(bar.chars().count(), 5);
    }
}
