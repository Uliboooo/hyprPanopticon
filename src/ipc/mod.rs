pub mod events;
pub mod snapshot;

/// Parse a Hyprland window address ("0x5693…") into its numeric form.
pub fn parse_address(s: &str) -> Option<u64> {
    let hex = s.strip_prefix("0x").unwrap_or(s);
    u64::from_str_radix(hex, 16).ok()
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_addresses() {
        assert_eq!(super::parse_address("0x5693171ffe00"), Some(0x5693171ffe00));
        assert_eq!(super::parse_address("5693171ffe00"), Some(0x5693171ffe00));
        assert_eq!(super::parse_address("nope"), None);
    }
}
