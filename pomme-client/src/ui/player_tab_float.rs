/// Format a scoreboard score as Java's `Float.toString(score / 2.0f)`.
/// The domain is specifically i32 scores converted to f32 before halving.
pub(super) fn java_half_heart_text(score: i32) -> String {
    let value = (score as f32) / 2.0f32;
    if value.abs() < 10_000_000.0 {
        return format!("{value:.1}");
    }

    for fractional_digits in 1..=8 {
        let candidate = format!("{value:.fractional_digits$e}");
        if candidate.parse::<f32>().ok() == Some(value) {
            return normalize_scientific(candidate);
        }
    }
    unreachable!("all finite f32 values round-trip with nine significant digits")
}

fn normalize_scientific(candidate: String) -> String {
    let (mantissa, exponent) = candidate.split_once('e').expect("Rust lowerExp output");
    let exponent: i32 = exponent.parse().expect("Rust exponent");
    format!("{mantissa}E{exponent}")
}

#[cfg(test)]
mod tests {
    use super::java_half_heart_text;

    #[test]
    fn matches_primary_jvm_golden() {
        let mut count = 0;
        for line in include_str!("player_tab_float_golden.tsv").lines() {
            let (score, expected) = line.split_once('\t').expect("score<TAB>text");
            assert_eq!(
                java_half_heart_text(score.parse().unwrap()),
                expected,
                "score={score}"
            );
            count += 1;
        }
        assert!(count >= 1_000, "golden sample unexpectedly small: {count}");
    }
}
