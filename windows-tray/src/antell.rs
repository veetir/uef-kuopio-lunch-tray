//! Antell HTML parsing helpers.

use crate::format::normalize_text;
use crate::model::{MenuGroup, TodayMenu};
use html_escape::decode_html_entities;
use scraper::{Html, Selector};

fn element_text(element: &scraper::element_ref::ElementRef) -> String {
    let raw = element.text().collect::<Vec<_>>().join(" ");
    let decoded = decode_html_entities(&raw);
    normalize_text(decoded.as_ref())
}

/// Parses an Antell lunch page into the normalized menu format used by the popup.
pub fn parse_antell_html(html: &str, today_key: &str) -> TodayMenu {
    let document = Html::parse_document(html);
    let section_sel = Selector::parse("section.menu-section").unwrap();
    let title_sel = Selector::parse("h2.menu-title").unwrap();
    let price_sel = Selector::parse("h2.menu-price").unwrap();
    let item_sel = Selector::parse("ul.menu-list > li").unwrap();

    let mut menus = Vec::new();

    for section in document.select(&section_sel) {
        let items: Vec<String> = section
            .select(&item_sel)
            .map(|item| {
                let raw = item.text().collect::<Vec<_>>().join(" ");
                let decoded = decode_html_entities(&raw);
                normalize_text(decoded.as_ref())
            })
            .filter(|text| !text.is_empty())
            .collect();
        if items.is_empty() {
            continue;
        }

        let name = section
            .select(&title_sel)
            .next()
            .map(|el| element_text(&el))
            .unwrap_or_else(|| "Menu".to_string());
        let price = section
            .select(&price_sel)
            .next()
            .map(|el| element_text(&el))
            .unwrap_or_default();

        menus.push(MenuGroup {
            name,
            price,
            components: items,
        });
    }

    TodayMenu {
        date_iso: today_key.to_string(),
        lunch_time: String::new(),
        menus,
    }
}
