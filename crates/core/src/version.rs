use std::cmp::Ordering;

/// Precedence tier for pre-release tags and release types.
/// In SemVer, final stable releases have higher precedence than any pre-release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReleaseTier {
    Snapshot = 10,
    Alpha = 20,
    Beta = 30,
    PreRelease = 40,
    ReleaseCandidate = 50,
    Stable = 100,
}

/// A parsed version token suitable for accurate, natural comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionToken {
    pub numbers: Vec<u64>,
    pub tier: ReleaseTier,
    pub tag_num: u64,
    pub raw_suffix: String,
}

impl VersionToken {
    /// Parses a version string into a structured `VersionToken`.
    pub fn parse(s: &str) -> Self {
        let trimmed = s.trim().trim_start_matches(['v', 'V']);
        if trimmed.is_empty() {
            return Self {
                numbers: Vec::new(),
                tier: ReleaseTier::Stable,
                tag_num: 0,
                raw_suffix: String::new(),
            };
        }

        // Special handling for Minecraft snapshot format, e.g. "24w45a"
        if let Some(token) = try_parse_minecraft_snapshot(trimmed) {
            return token;
        }

        // Split into numeric dot-separated prefix and trailing suffix (-pre4, -rc1, etc.)
        let (num_part, suffix_part) =
            match trimmed.find(|c: char| c == '-' || c == '+' || (c.is_alphabetic() && c != '.')) {
                Some(idx) => (&trimmed[..idx], &trimmed[idx..]),
                None => (trimmed, ""),
            };

        let numbers: Vec<u64> = num_part
            .split('.')
            .filter_map(|p| p.parse::<u64>().ok())
            .collect();

        let (tier, tag_num, raw_suffix) = parse_suffix(suffix_part);

        Self {
            numbers,
            tier,
            tag_num,
            raw_suffix,
        }
    }
}

impl PartialOrd for VersionToken {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for VersionToken {
    fn cmp(&self, other: &Self) -> Ordering {
        // 1. Compare numeric components segment by segment
        let max_len = self.numbers.len().max(other.numbers.len());
        for i in 0..max_len {
            let n1 = self.numbers.get(i).copied().unwrap_or(0);
            let n2 = other.numbers.get(i).copied().unwrap_or(0);
            if n1 != n2 {
                return n1.cmp(&n2);
            }
        }

        // 2. If numeric prefix is identical, compare release tier (Stable > RC > Pre > Beta > Alpha > Snapshot)
        match self.tier.cmp(&other.tier) {
            Ordering::Equal => {}
            non_eq => return non_eq,
        }

        // 3. If tier is identical, compare tag numbers (e.g. pre4 > pre3)
        match self.tag_num.cmp(&other.tag_num) {
            Ordering::Equal => {}
            non_eq => return non_eq,
        }

        // 4. Suffix fallback comparison
        self.raw_suffix.cmp(&other.raw_suffix)
    }
}

/// Parses Minecraft snapshot format like "24w45a" into (year 24, week 45, rev a)
fn try_parse_minecraft_snapshot(s: &str) -> Option<VersionToken> {
    let w_pos = s.find('w')?;
    if w_pos == 0 || w_pos + 1 >= s.len() {
        return None;
    }
    let year: u64 = s[..w_pos].parse().ok()?;
    let rest = &s[w_pos + 1..];
    let num_end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    let week: u64 = rest[..num_end].parse().ok()?;
    let rev_char = rest[num_end..].chars().next().unwrap_or('a');
    let rev_num = (rev_char.to_ascii_lowercase() as u64).saturating_sub('a' as u64) + 1;

    Some(VersionToken {
        numbers: vec![year, week, rev_num],
        tier: ReleaseTier::Snapshot,
        tag_num: rev_num,
        raw_suffix: s.to_string(),
    })
}

/// Analyzes suffix part (e.g. "-pre4", "-rc1", "-SNAPSHOT") to determine release tier and sub-tag number.
fn parse_suffix(suffix: &str) -> (ReleaseTier, u64, String) {
    let clean = suffix.trim_start_matches(['-', '+', '.']);
    if clean.is_empty() {
        return (ReleaseTier::Stable, 0, String::new());
    }

    let lower = clean.to_lowercase();
    let num = extract_trailing_number(&lower);

    let tier = if lower.contains("rc") {
        ReleaseTier::ReleaseCandidate
    } else if lower.contains("pre") {
        ReleaseTier::PreRelease
    } else if lower.contains("beta") || lower.starts_with('b') {
        ReleaseTier::Beta
    } else if lower.contains("alpha") || lower.starts_with('a') {
        ReleaseTier::Alpha
    } else if lower.contains("snap") {
        ReleaseTier::Snapshot
    } else {
        ReleaseTier::Beta
    };

    (tier, num, clean.to_string())
}

/// Extracts trailing or embedded digits from a suffix string (e.g. "pre4" -> 4, "rc.2" -> 2)
fn extract_trailing_number(s: &str) -> u64 {
    let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    digits.parse().unwrap_or(0)
}

/// Compares two version strings in natural, semantic ascending order.
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    VersionToken::parse(a).cmp(&VersionToken::parse(b))
}

/// Sorts a slice of version strings in descending order (latest version first).
pub fn sort_versions_descending(versions: &mut [String]) {
    versions.sort_by(|a, b| compare_versions(b, a));
}

/// Returns true if the version string represents a final stable release (not a pre-release, RC, or snapshot).
pub fn is_stable_version(v: &str) -> bool {
    VersionToken::parse(v).tier == ReleaseTier::Stable
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semver_numeric_ordering() {
        assert_eq!(compare_versions("1.21.4", "1.9.4"), Ordering::Greater);
        assert_eq!(compare_versions("1.21.10", "1.21.9"), Ordering::Greater);
        assert_eq!(compare_versions("1.21", "1.20.4"), Ordering::Greater);
        assert_eq!(compare_versions("1.21.0", "1.21"), Ordering::Equal);
        assert_eq!(compare_versions("1.8.8", "1.7.10"), Ordering::Greater);
        assert_eq!(compare_versions("1.9.4", "1.8.8"), Ordering::Greater);
    }

    #[test]
    fn test_prerelease_precedence() {
        // Stable release must be newer than pre-release of the same version
        assert_eq!(compare_versions("1.21.9", "1.21.9-pre4"), Ordering::Greater);
        assert_eq!(
            compare_versions("1.21.9-pre4", "1.21.9-pre3"),
            Ordering::Greater
        );
        assert_eq!(
            compare_versions("1.21.9-pre2", "1.21.9-pre1"),
            Ordering::Greater
        );

        // RC must be newer than pre-release
        assert_eq!(
            compare_versions("1.21.9-rc1", "1.21.9-pre4"),
            Ordering::Greater
        );
        assert_eq!(
            compare_versions("1.21.9-rc2", "1.21.9-rc1"),
            Ordering::Greater
        );
        assert_eq!(compare_versions("1.21.9", "1.21.9-rc2"), Ordering::Greater);
    }

    #[test]
    fn test_bedrock_four_part_versions() {
        assert_eq!(
            compare_versions("1.21.60.21", "1.21.51.2"),
            Ordering::Greater
        );
        assert_eq!(
            compare_versions("1.21.51.02", "1.21.50.01"),
            Ordering::Greater
        );
    }

    #[test]
    fn test_minecraft_snapshots() {
        assert_eq!(compare_versions("24w45a", "24w44a"), Ordering::Greater);
        assert_eq!(compare_versions("24w45b", "24w45a"), Ordering::Greater);
    }

    #[test]
    fn test_sort_versions_descending() {
        let mut list = vec![
            "1.9.4".to_string(),
            "1.8.8".to_string(),
            "1.7.10".to_string(),
            "1.21.9-pre4".to_string(),
            "1.21.9-pre3".to_string(),
            "1.21.9-pre2".to_string(),
            "1.21.9".to_string(),
            "1.21.8".to_string(),
            "1.21.10".to_string(),
        ];

        sort_versions_descending(&mut list);

        assert_eq!(
            list,
            vec![
                "1.21.10",
                "1.21.9",
                "1.21.9-pre4",
                "1.21.9-pre3",
                "1.21.9-pre2",
                "1.21.8",
                "1.9.4",
                "1.8.8",
                "1.7.10",
            ]
        );
    }

    #[test]
    fn test_is_stable_version() {
        assert!(is_stable_version("1.21.4"));
        assert!(is_stable_version("1.9.4"));
        assert!(!is_stable_version("1.21.9-pre4"));
        assert!(!is_stable_version("1.21.9-rc1"));
        assert!(!is_stable_version("3.3.0-SNAPSHOT"));
        assert!(!is_stable_version("24w45a"));
    }
}
