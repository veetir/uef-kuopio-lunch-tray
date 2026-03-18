use crate::model::{MenuGroup, TodayMenu};
use crate::restaurant::Provider;

#[derive(Debug, Clone, Copy)]
pub struct PriceGroups {
    pub student: bool,
    pub staff: bool,
    pub guest: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PriceGroup {
    Student,
    Staff,
    Guest,
}

#[derive(Debug, Clone)]
struct PriceEntry {
    group: PriceGroup,
    text: String,
    value: Option<f32>,
}

pub fn normalize_text(value: &str) -> String {
    let mut out = String::new();
    let mut last_was_space = false;
    for ch in value.chars() {
        let is_space = ch.is_whitespace();
        if is_space {
            if !last_was_space {
                out.push(' ');
                last_was_space = true;
            }
        } else {
            out.push(ch);
            last_was_space = false;
        }
    }
    out.trim().to_string()
}

pub fn normalize_optional(value: Option<&str>) -> String {
    match value {
        Some(v) => normalize_text(v),
        None => String::new(),
    }
}

pub fn format_display_date(date_iso: &str, language: &str) -> String {
    let iso = normalize_text(date_iso);
    let parts: Vec<&str> = iso.split('-').collect();
    if parts.len() != 3 {
        return iso;
    }
    let year = parts[0];
    let month = match parts[1].parse::<u32>() {
        Ok(m) => m,
        Err(_) => return iso,
    };
    let day = match parts[2].parse::<u32>() {
        Ok(d) => d,
        Err(_) => return iso,
    };
    if language == "fi" {
        return format!("{}.{}.{}", day, month, year);
    }
    format!("{}/{}/{}", month, day, year)
}

pub fn date_and_time_line(today_menu: Option<&TodayMenu>, language: &str) -> String {
    let menu = match today_menu {
        Some(m) => m,
        None => return String::new(),
    };
    let date_part = format_display_date(&menu.date_iso, language);
    let time_part = normalize_text(&menu.lunch_time);
    if !date_part.is_empty() && !time_part.is_empty() {
        format!("{} {}", date_part, time_part)
    } else if !date_part.is_empty() {
        date_part
    } else {
        time_part
    }
}

pub fn text_for(language: &str, key: &str) -> String {
    if language == "fi" {
        match key {
            "loading" => "Ladataan ruokalistaa...".to_string(),
            "noMenu" => "Tälle päivälle ei ole lounaslistaa.".to_string(),
            "stale" => "Päivitys epäonnistui. Näytetään viimeisin tallennettu lista.".to_string(),
            "staleNetwork" => {
                "Ei verkkoyhteyttä. Näytetään viimeisin tallennettu lista.".to_string()
            }
            "fetchError" => "Päivitysvirhe".to_string(),
            _ => key.to_string(),
        }
    } else {
        match key {
            "loading" => "Loading menu...".to_string(),
            "noMenu" => "No lunch menu available for today.".to_string(),
            "stale" => "Update failed. Showing last cached menu.".to_string(),
            "staleNetwork" => "Offline. Showing last cached menu.".to_string(),
            "fetchError" => "Fetch error".to_string(),
            _ => key.to_string(),
        }
    }
}

pub fn menu_heading(
    menu: &MenuGroup,
    provider: Provider,
    show_prices: bool,
    groups: PriceGroups,
) -> String {
    let mut heading = normalize_text(&menu.name);
    if heading.is_empty() {
        heading = "Menu".to_string();
    }
    let price = normalize_text(&menu.price);
    if show_prices && !price.is_empty() {
        if provider == Provider::Compass {
            let filtered = price_text_for_groups(&price, groups);
            if filtered.is_empty() {
                heading
            } else {
                format!("{} - {}", heading, filtered)
            }
        } else {
            format!("{} - {}", heading, price)
        }
    } else {
        heading
    }
}

pub fn split_component_suffix(component: &str) -> (String, String) {
    let text = normalize_text(component);
    if text.is_empty() {
        return (String::new(), String::new());
    }
    let mut main = text.trim().to_string();
    let mut trailing_group_tokens = extract_trailing_parenthesized_allergens(&mut main);
    let (inline_main, mut inline_tokens) = extract_inline_allergens(&main);
    if !inline_tokens.is_empty() {
        main = inline_main;
    }
    inline_tokens.append(&mut trailing_group_tokens);
    let tokens = dedupe_tokens(inline_tokens);
    let normalized_main = clean_main_text(&main);

    if tokens.is_empty() {
        return (normalized_main, String::new());
    }

    let suffix = format!("({})", tokens.join(", "));
    (normalized_main, suffix)
}

pub fn renderable_menu_components(group: &MenuGroup) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for component in &group.components {
        let component = normalize_text(component);
        if component.is_empty() {
            continue;
        }
        let (main, suffix) = split_component_suffix(&component);
        if main.is_empty() {
            continue;
        }
        out.push((main, suffix));
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ParenthesizedGroup {
    Tokens(Vec<String>),
    Empty,
    Invalid,
}

fn extract_trailing_parenthesized_allergens(main: &mut String) -> Vec<String> {
    let mut groups_rev: Vec<Vec<String>> = Vec::new();

    loop {
        let trimmed = main.trim_end();
        if !trimmed.ends_with(')') {
            break;
        }

        let start = match find_matching_open_paren(trimmed) {
            Some(value) => value,
            None => break,
        };
        let end = trimmed.len().saturating_sub(1);
        let inside = &trimmed[start + 1..end];
        match parse_parenthesized_group_tokens(inside) {
            ParenthesizedGroup::Tokens(tokens) => {
                groups_rev.push(tokens);
                *main = trimmed[..start].trim_end().to_string();
            }
            ParenthesizedGroup::Empty => {
                // Drop empty trailing groups like "()".
                *main = trimmed[..start].trim_end().to_string();
            }
            ParenthesizedGroup::Invalid => break,
        }
    }

    groups_rev.into_iter().rev().flatten().collect()
}

fn find_matching_open_paren(value: &str) -> Option<usize> {
    let mut depth: i32 = 0;
    for (idx, ch) in value.char_indices().rev() {
        if ch == ')' {
            depth += 1;
        } else if ch == '(' {
            depth -= 1;
            if depth == 0 {
                return Some(idx);
            }
            if depth < 0 {
                return None;
            }
        }
    }
    None
}

fn parse_parenthesized_group_tokens(raw_inside: &str) -> ParenthesizedGroup {
    let inside = normalize_text(raw_inside);
    if inside.is_empty() {
        return ParenthesizedGroup::Empty;
    }

    let parts: Vec<&str> = if inside.contains(',') {
        inside.split(',').collect()
    } else {
        inside.split_whitespace().collect()
    };

    let mut tokens = Vec::new();
    for part in parts {
        let clean = normalize_text(part);
        if clean.is_empty() {
            continue;
        }
        let Some(token) = normalize_allergen_token(&clean) else {
            return ParenthesizedGroup::Invalid;
        };
        tokens.push(token);
    }

    if tokens.is_empty() {
        ParenthesizedGroup::Empty
    } else {
        ParenthesizedGroup::Tokens(tokens)
    }
}

fn extract_inline_allergens(text: &str) -> (String, Vec<String>) {
    let compact = normalize_text(text)
        .trim_end_matches([' ', ',', ';', ':', '.'])
        .to_string();
    if compact.is_empty() {
        return (String::new(), Vec::new());
    }

    let parts: Vec<String> = compact
        .split(',')
        .map(normalize_text)
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() < 2 {
        return (compact, Vec::new());
    }

    let mut suffix_tokens = Vec::new();
    for idx in (0..parts.len()).rev() {
        let candidate = normalize_text(&parts[idx]);
        let Some(token) = normalize_allergen_token(&candidate) else {
            break;
        };
        suffix_tokens.insert(0, token);
    }
    if suffix_tokens.is_empty() {
        return (compact, Vec::new());
    }

    let mut main =
        normalize_text(&parts[..parts.len().saturating_sub(suffix_tokens.len())].join(", "));
    if main.is_empty() {
        return (compact, Vec::new());
    }

    while let Some((next_main, token)) = peel_last_allergen_token(&main) {
        if next_main.is_empty() {
            break;
        }
        main = next_main;
        suffix_tokens.insert(0, token);
    }

    if main.is_empty() {
        (compact, Vec::new())
    } else {
        (main, suffix_tokens)
    }
}

fn peel_last_allergen_token(text: &str) -> Option<(String, String)> {
    let trimmed = text.trim_end();
    let split_idx = trimmed.rfind(|ch: char| ch.is_whitespace())?;
    let prefix = normalize_text(&trimmed[..split_idx]);
    let candidate = normalize_text(&trimmed[split_idx + 1..]);
    let token = normalize_allergen_token(&candidate)?;
    if prefix.is_empty() {
        None
    } else {
        Some((prefix, token))
    }
}

fn normalize_allergen_token(token: &str) -> Option<String> {
    let clean = normalize_text(token)
        .trim_matches(['(', ')', ',', ';', ':', '.'])
        .to_string();
    if clean.is_empty() {
        return None;
    }
    if clean == "*" {
        return Some("*".to_string());
    }

    let upper = clean.to_ascii_uppercase();
    if upper.len() == 1 && upper.chars().all(|ch| ch.is_ascii_uppercase()) {
        return Some(upper);
    }

    match upper.as_str() {
        "ILM" | "VS" | "VL" => Some(upper),
        "VEG" => Some("Veg".to_string()),
        _ => None,
    }
}

fn dedupe_tokens(tokens: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for token in tokens {
        let key = token.to_ascii_uppercase();
        if seen.iter().any(|entry| entry == &key) {
            continue;
        }
        seen.push(key);
        out.push(token);
    }
    out
}

fn clean_main_text(main: &str) -> String {
    normalize_text(main)
        .trim_end_matches([' ', ',', ';', ':'])
        .to_string()
}

pub fn student_price_eur(price: &str) -> Option<f32> {
    let entries = parse_compass_price_entries(price);
    entries
        .into_iter()
        .find(|entry| entry.group == PriceGroup::Student)
        .and_then(|entry| entry.value)
}

fn price_text_for_groups(price: &str, groups: PriceGroups) -> String {
    let entries = parse_compass_price_entries(price);
    let mut parts = Vec::new();
    for entry in entries {
        let include = match entry.group {
            PriceGroup::Student => groups.student,
            PriceGroup::Staff => groups.staff,
            PriceGroup::Guest => groups.guest,
        };
        if include {
            parts.push(entry.text);
        }
    }
    parts.join(" / ")
}

fn parse_compass_price_entries(price: &str) -> Vec<PriceEntry> {
    let normalized = normalize_text(price);
    if normalized.is_empty() {
        return Vec::new();
    }
    split_compass_price_segments(&normalized)
        .into_iter()
        .map(|segment| PriceEntry {
            group: classify_compass_price_group(&segment),
            value: parse_price_value(&segment),
            text: segment,
        })
        .collect()
}

fn split_compass_price_segments(price: &str) -> Vec<String> {
    let slash_segments: Vec<String> = price
        .split('/')
        .map(normalize_text)
        .filter(|segment| !segment.is_empty())
        .collect();
    if slash_segments.len() > 1 {
        return slash_segments;
    }

    let starts = group_label_starts(price);
    if starts.len() <= 1 {
        return slash_segments
            .into_iter()
            .next()
            .map(|segment| vec![segment])
            .unwrap_or_else(|| vec![price.to_string()]);
    }

    let mut segments = Vec::new();
    for (idx, start) in starts.iter().enumerate() {
        let end = starts.get(idx + 1).copied().unwrap_or(price.len());
        let segment = normalize_text(&price[*start..end]);
        if !segment.is_empty() {
            segments.push(segment);
        }
    }

    if segments.is_empty() {
        vec![price.to_string()]
    } else {
        segments
    }
}

fn classify_compass_price_group(segment: &str) -> PriceGroup {
    let lower = segment.to_lowercase();
    if has_any_word_label(&lower, &["student", "op", "opisk", "opiskelija"]) {
        PriceGroup::Student
    } else if has_any_word_label(&lower, &["staff", "hk", "henkilokunta", "henkilökunta"]) {
        PriceGroup::Staff
    } else {
        PriceGroup::Guest
    }
}

fn group_label_starts(text: &str) -> Vec<usize> {
    let lower = text.to_lowercase();
    let mut starts = Vec::new();
    for label in [
        "student",
        "staff",
        "guest",
        "opiskelija",
        "opisk",
        "op",
        "henkilokunta",
        "henkilökunta",
        "hk",
        "vieras",
    ] {
        for (start, _) in lower.match_indices(label) {
            if is_word_boundary(&lower, start, label.len()) {
                starts.push(start);
            }
        }
    }
    starts.sort_unstable();
    starts.dedup();
    starts
}

fn has_any_word_label(text: &str, labels: &[&str]) -> bool {
    labels.iter().any(|label| {
        text.match_indices(label)
            .any(|(idx, _)| is_word_boundary(text, idx, label.len()))
    })
}

fn is_word_boundary(text: &str, start: usize, len: usize) -> bool {
    let prev_ok = text[..start]
        .chars()
        .next_back()
        .map(|ch| !ch.is_alphabetic())
        .unwrap_or(true);
    let end = start + len;
    let next_ok = text[end..]
        .chars()
        .next()
        .map(|ch| !ch.is_alphabetic())
        .unwrap_or(true);
    prev_ok && next_ok
}

fn parse_price_value(text: &str) -> Option<f32> {
    let mut current = String::new();
    let mut tokens: Vec<String> = Vec::new();
    for ch in text.chars() {
        if ch.is_ascii_digit() || ch == ',' || ch == '.' {
            current.push(ch);
        } else if !current.is_empty() {
            tokens.push(current.clone());
            current.clear();
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    let token = tokens.last()?.replace(',', ".");
    let cleaned = token.trim_matches('.');
    cleaned.parse::<f32>().ok()
}

#[cfg(test)]
mod tests {
    use super::{renderable_menu_components, split_component_suffix};
    use crate::model::MenuGroup;

    #[test]
    fn extracts_compass_suffix_with_parentheses() {
        let (main, suffix) = split_component_suffix(
            "Organic tofu and vegetables in teriyaki sauce (*, A, G, ILM, L, M, Veg, VS)",
        );
        assert_eq!(main, "Organic tofu and vegetables in teriyaki sauce");
        assert_eq!(suffix, "(*, A, G, ILM, L, M, Veg, VS)");
    }

    #[test]
    fn extracts_suffix_when_newline_precedes_parentheses() {
        let (main, suffix) = split_component_suffix(
            "Roasted rainbow trout in teriyaki sauce\n (*, A, G, ILM, L, M, VS)",
        );
        assert_eq!(main, "Roasted rainbow trout in teriyaki sauce");
        assert_eq!(suffix, "(*, A, G, ILM, L, M, VS)");
    }

    #[test]
    fn extracts_inline_suffix_without_parentheses() {
        let (main, suffix) =
            split_component_suffix("Chili and sesame-spiced organic tofu A, ILM, L, M, Veg, VS");
        assert_eq!(main, "Chili and sesame-spiced organic tofu");
        assert_eq!(suffix, "(A, ILM, L, M, Veg, VS)");
    }

    #[test]
    fn removes_empty_trailing_group_and_normalizes_spacing() {
        let (main, suffix) = split_component_suffix("Juustoista pinaattikastiketta ( A, L) ()");
        assert_eq!(main, "Juustoista pinaattikastiketta");
        assert_eq!(suffix, "(A, L)");
    }

    #[test]
    fn keeps_non_allergen_tail_as_main_text() {
        let (main, suffix) = split_component_suffix("Juusto, edam, viipale, sk ()");
        assert_eq!(main, "Juusto, edam, viipale, sk");
        assert_eq!(suffix, "");
    }

    #[test]
    fn extracts_huomen_style_suffix_with_comma_in_main() {
        let (main, suffix) =
            split_component_suffix("Lihapullia, pippuri-rakuunakastiketta ja kermaperunaa (G, L)");
        assert_eq!(
            main,
            "Lihapullia, pippuri-rakuunakastiketta ja kermaperunaa"
        );
        assert_eq!(suffix, "(G, L)");
    }

    #[test]
    fn skips_empty_renderable_components() {
        let group = MenuGroup {
            name: "Lunch".to_string(),
            price: String::new(),
            components: vec![],
        };
        assert!(renderable_menu_components(&group).is_empty());
    }

    #[test]
    fn skips_blank_renderable_components() {
        let group = MenuGroup {
            name: "Lunch".to_string(),
            price: String::new(),
            components: vec!["".to_string(), "   ".to_string(), "\n\t".to_string()],
        };
        assert!(renderable_menu_components(&group).is_empty());
    }

    #[test]
    fn keeps_only_valid_renderable_components() {
        let group = MenuGroup {
            name: "Lunch".to_string(),
            price: String::new(),
            components: vec![
                "".to_string(),
                " ".to_string(),
                "Soup (L)".to_string(),
                "()".to_string(),
            ],
        };
        assert_eq!(
            renderable_menu_components(&group),
            vec![("Soup".to_string(), "(L)".to_string())]
        );
    }
}
