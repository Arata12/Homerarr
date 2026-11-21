use homarr_definitions::WidgetKind;
use maud::{html, Markup, PreEscaped};
use serde_json::Value;
use url::Url;

use crate::registry::get_widget_metadata;

/// Filters unsafe URL schemes (`javascript:`, `data:text/html`, protocol-relative `//`, quotes)
/// in rendered anchors/images/frames.
pub fn safe_url(raw: &str) -> &str {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.starts_with("//")
        || trimmed
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>'))
    {
        return "#";
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") || trimmed.starts_with('/') {
        trimmed
    } else {
        "#"
    }
}

fn host_port_key(url: &Url) -> Option<String> {
    let host = url.host_str()?.to_ascii_lowercase();
    Some(match url.port_or_known_default() {
        Some(p) => format!("{host}:{p}"),
        None => host,
    })
}

fn is_same_origin_url(safe: &str, self_host_or_origin: Option<&str>) -> bool {
    let Ok(target) = Url::parse(safe) else {
        return false;
    };
    let check_against = |candidate: &str| -> bool {
        let trimmed = candidate.trim();
        if trimmed.is_empty() {
            return false;
        }
        if let Ok(origin_url) = Url::parse(trimmed) {
            return target.origin() == origin_url.origin();
        }
        // Host[:port] without scheme (e.g., from Host / X-Forwarded-Host header): compare host+port
        if let Ok(synthetic) = Url::parse(&format!("{}://{trimmed}", target.scheme())) {
            return host_port_key(&target) == host_port_key(&synthetic);
        }
        false
    };

    if self_host_or_origin.is_some_and(check_against) {
        return true;
    }
    if let Ok(base) = std::env::var("BASE_URL") {
        if check_against(&base) {
            return true;
        }
    }
    false
}

fn safe_hex_color(raw: &str) -> &str {
    let trimmed = raw.trim();
    if trimmed.starts_with('#')
        && (trimmed.len() == 4 || trimmed.len() == 7)
        && trimmed[1..].chars().all(|c| c.is_ascii_hexdigit())
    {
        trimmed
    } else {
        "#339af0"
    }
}

fn format_compact_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

fn format_speed(bps: u64) -> String {
    if bps >= 1_000_000_000 {
        format!("{:.1} GB/s", bps as f64 / 1_000_000_000.0)
    } else if bps >= 1_000_000 {
        format!("{:.1} MB/s", bps as f64 / 1_000_000.0)
    } else if bps >= 1_000 {
        format!("{:.0} KB/s", bps as f64 / 1_000.0)
    } else {
        format!("{bps} B/s")
    }
}

fn format_si_bytes(bytes: u64) -> String {
    if bytes >= 1_000_000_000_000 {
        format!("{:.1} TB", bytes as f64 / 1_000_000_000_000.0)
    } else if bytes >= 1_000_000_000 {
        format!("{:.0} GB", bytes as f64 / 1_000_000_000.0)
    } else if bytes >= 1_000_000 {
        format!("{:.0} MB", bytes as f64 / 1_000_000.0)
    } else {
        format!("{bytes} B")
    }
}

fn format_gib(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / 1_073_741_824.0)
}

fn format_uptime(secs: u64) -> String {
    let days = secs / 86_400;
    let hours = (secs % 86_400) / 3_600;
    if days > 0 {
        format!("{days}d {hours}h")
    } else {
        let mins = (secs % 3_600) / 60;
        format!("{hours}h {mins}m")
    }
}

fn format_eta(secs: u64) -> String {
    if secs >= 3600 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else if secs >= 60 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{secs}s")
    }
}

fn weather_visual(code: i64, is_day: bool) -> (&'static str, &'static str) {
    match code {
        0 => {
            if is_day {
                (
                    "Clear sky",
                    r#"<svg class="weather-svg weather-anim-sun" viewBox="0 0 24 24" width="44" height="44" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="4" /><path d="M12 2v2m0 16v2M4.93 4.93l1.41 1.41m11.32 11.32l1.41 1.41M2 12h2m16 0h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41" /></svg>"#,
                )
            } else {
                (
                    "Clear night",
                    r#"<svg class="weather-svg weather-anim-moon" viewBox="0 0 24 24" width="44" height="44" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3c.132 0 .263 0 .393 0a7.5 7.5 0 0 0 7.92 12.446a9 9 0 1 1 -8.313 -12.454z" /></svg>"#,
                )
            }
        }
        1..=3 | 45 | 48 => (
            if code <= 2 { "Partly cloudy" } else { "Cloudy" },
            r#"<svg class="weather-svg weather-anim-cloud" viewBox="0 0 24 24" width="44" height="44" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M6.657 18c-2.572 0 -4.657 -2.007 -4.657 -4.483c0 -2.475 2.085 -4.482 4.657 -4.482c.393 -1.762 1.794 -3.2 3.675 -3.773c1.88 -.572 3.956 -.193 5.444 1c1.488 1.19 2.162 3.007 1.77 4.769h.99c1.913 0 3.464 1.56 3.464 3.486c0 1.927 -1.551 3.487 -3.465 3.487h-11.878" /></svg>"#,
        ),
        51..=67 | 80..=82 => (
            "Rain",
            r#"<svg class="weather-svg weather-anim-rain" viewBox="0 0 24 24" width="44" height="44" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M7 18a4.6 4.4 0 0 1 0 -9a5 4.5 0 0 1 11 2h1a3.5 3.5 0 0 1 0 7" /><path d="M11 13v2m0 3v2m4 -5v2m0 3v2" /></svg>"#,
        ),
        71..=77 | 85..=86 => (
            "Snow",
            r#"<svg class="weather-svg weather-anim-snow" viewBox="0 0 24 24" width="44" height="44" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M7 18a4.6 4.4 0 0 1 0 -9a5 4.5 0 0 1 11 2h1a3.5 3.5 0 0 1 0 7" /><path d="M11 15v.01m0 3v.01m0 3v.01m4 -4v.01m0 3v.01" /></svg>"#,
        ),
        95..=99 => (
            "Thunderstorm",
            r#"<svg class="weather-svg weather-anim-storm" viewBox="0 0 24 24" width="44" height="44" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M7 18a4.6 4.4 0 0 1 0 -9a5 4.5 0 0 1 11 2h1a3.5 3.5 0 0 1 0 7h-1" /><path d="M13 14l-2 4l3 0l-2 4" /></svg>"#,
        ),
        _ => (
            "Overcast",
            r#"<svg class="weather-svg weather-anim-cloud" viewBox="0 0 24 24" width="44" height="44" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M6.657 18c-2.572 0 -4.657 -2.007 -4.657 -4.483c0 -2.475 2.085 -4.482 4.657 -4.482c.393 -1.762 1.794 -3.2 3.675 -3.773c1.88 -.572 3.956 -.193 5.444 1c1.488 1.19 2.162 3.007 1.77 4.769h.99c1.913 0 3.464 1.56 3.464 3.486c0 1.927 -1.551 3.487 -3.465 3.487h-11.878" /></svg>"#,
        ),
    }
}

fn widget_header_icon(kind: WidgetKind) -> &'static str {
    use WidgetKind as W;
    match kind {
        W::Downloads => {
            r#"<svg class="ui-icon" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M4 17v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2 -2v-2" /><path d="M7 11l5 5l5 -5" /><path d="M12 4l0 12" /></svg>"#
        }
        W::MediaServer => {
            r#"<svg class="ui-icon" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M7 4v16l13 -8z" /></svg>"#
        }
        W::HealthMonitoring | W::SystemResources | W::SystemDisks => {
            r#"<svg class="ui-icon" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12h4l3 8l4 -16l3 8h4" /></svg>"#
        }
        W::Calendar => {
            r#"<svg class="ui-icon" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M4 7a2 2 0 0 1 2 -2h12a2 2 0 0 1 2 2v12a2 2 0 0 1 -2 2h-12a2 2 0 0 1 -2 -2v-12z" /><path d="M16 3v4" /><path d="M8 3v4" /><path d="M4 11h16" /></svg>"#
        }
        W::Bookmarks => {
            r#"<svg class="ui-icon" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M18 7v14l-6 -4l-6 4v-14a4 4 0 0 1 4 -4h4a4 4 0 0 1 4 4z" /></svg>"#
        }
        W::SmartHomeEntityState | W::SmartHomeExecuteAutomation | W::SmartHomeControls => {
            r#"<svg class="ui-icon" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12l-2 0l9 -9l9 9l-2 0" /><path d="M5 12v7a2 2 0 0 0 2 2h10a2 2 0 0 0 2 -2v-7" /><path d="M10 12h4v4h-4z" /></svg>"#
        }
        W::DockerContainers => {
            r#"<svg class="ui-icon" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3l8 4.5v9l-8 4.5l-8 -4.5v-9z" /></svg>"#
        }
        W::RssFeed => {
            r#"<svg class="ui-icon" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M5 19m-1 0a1 1 0 1 0 2 0a1 1 0 1 0 -2 0" /><path d="M4 4a16 16 0 0 1 16 16" /><path d="M4 11a9 9 0 0 1 9 9" /></svg>"#
        }
        _ => {
            r#"<svg class="ui-icon" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M4 4h6v6h-6z" /><path d="M14 4h6v6h-6z" /><path d="M4 14h6v6h-6z" /><path d="M14 14h6v6h-6z" /></svg>"#
        }
    }
}

fn empty_box(msg: &str) -> Markup {
    html! { div.widget-empty { (msg) } }
}

/// Renders an XSS-safe HTML card body for any `WidgetKind`.
pub fn render_widget_html(
    kind: WidgetKind,
    item_id: &str,
    options: &Value,
    data: &Value,
) -> String {
    render_widget_html_with_origin(kind, item_id, options, data, None)
}

/// Renders an XSS-safe HTML card body for any `WidgetKind`, stripping `allow-same-origin`
/// on `iframe` embeds that target `self_origin` (origin URL or `host[:port]`).
pub fn render_widget_html_with_origin(
    kind: WidgetKind,
    item_id: &str,
    options: &Value,
    data: &Value,
    self_origin: Option<&str>,
) -> String {
    render_widget_markup(kind, item_id, options, data, self_origin).into_string()
}

/// Same as [`render_widget_html_with_origin`], returned as Maud `Markup` for embedding in
/// server-rendered pages without a raw-HTML wrapper.
pub fn render_widget_markup(
    kind: WidgetKind,
    item_id: &str,
    options: &Value,
    data: &Value,
    self_origin: Option<&str>,
) -> Markup {
    render_widget(kind, item_id, options, data, self_origin)
}

fn render_widget(
    kind: WidgetKind,
    item_id: &str,
    options: &Value,
    data: &Value,
    self_origin: Option<&str>,
) -> Markup {
    let meta = get_widget_metadata(kind);
    let custom_title = options
        .get("title")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty());
    let title = custom_title.unwrap_or(&meta.name);
    // safe: static SVG const.
    let icon = PreEscaped(widget_header_icon(kind));

    if data
        .get("missingIntegration")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return html! {
            div class={ "widget-shell widget-" (kind.as_str()) } data-item-id=(item_id) {
                div.widget-header {
                    span.widget-header-title { (icon) span { (title) } }
                    span.badge.muted { "No Integration" }
                }
                div.widget-empty { "Assign a compatible integration in board edit mode to display live telemetry." }
            }
        };
    }

    let body = match kind {
        WidgetKind::App => render_app(options, data),
        WidgetKind::Bookmarks => render_bookmarks(options),
        WidgetKind::Clock => render_clock(options, data),
        WidgetKind::Weather => render_weather(options, data),
        WidgetKind::AirQuality => render_air_quality(data),
        WidgetKind::Notebook => render_notebook(options),
        WidgetKind::Countdown => render_countdown(options),
        WidgetKind::Timer => render_timer(options),
        WidgetKind::Iframe => render_iframe(options, self_origin),
        WidgetKind::Video => render_video(options),
        WidgetKind::RssFeed => render_rss(data),
        WidgetKind::StockPrice => render_stock(data),
        WidgetKind::MinecraftServerStatus => render_minecraft(data),
        WidgetKind::Releases => render_release(data),
        WidgetKind::DockerContainers => render_docker(data),
        WidgetKind::Downloads => render_downloads(data),
        WidgetKind::MediaServer => render_media_sessions(data),
        WidgetKind::MediaRequestsRequestList | WidgetKind::MediaRequestsRequestStats => {
            render_media_requests(kind, data)
        }
        WidgetKind::Calendar => render_calendar(data),
        WidgetKind::DnsHoleSummary | WidgetKind::DnsHoleControls => render_dns_hole(kind, data),
        WidgetKind::HealthMonitoring | WidgetKind::SystemResources | WidgetKind::SystemDisks => {
            render_health(kind, data)
        }
        WidgetKind::SmartHomeEntityState
        | WidgetKind::SmartHomeExecuteAutomation
        | WidgetKind::SmartHomeControls => render_smart_home(data),
        WidgetKind::Notifications => render_notifications(data),
        WidgetKind::IndexerManager => render_indexers(data),
        WidgetKind::Ups => render_ups(data),
        WidgetKind::MediaMissing => render_media_missing(data),
        WidgetKind::MediaReleases => render_media_releases(data),
        WidgetKind::MediaTranscoding => render_media_transcoding(data),
        WidgetKind::Firewall => render_firewall(data),
        WidgetKind::Vpn => render_vpn(data),
        WidgetKind::Traefik => render_traefik(data),
        WidgetKind::NetworkControllerSummary | WidgetKind::NetworkControllerStatus => {
            render_network_controller(data)
        }
        WidgetKind::AnchorNote => render_anchor_note(data),
        WidgetKind::Stats => render_stats(data),
        WidgetKind::CustomApi => render_custom_api(data),
        WidgetKind::Assistant => render_assistant(options),
        _ if data.get("error").and_then(|v| v.as_str()).is_some() => render_generic(data),
        _ if data.get("stats").is_some() => render_stats_with_extras(kind, data),
        _ => render_generic(data),
    };

    let show_header = !matches!(
        kind,
        WidgetKind::App
            | WidgetKind::Clock
            | WidgetKind::Weather
            | WidgetKind::DnsHoleSummary
            | WidgetKind::AirQuality
            | WidgetKind::Iframe
            | WidgetKind::Countdown
            | WidgetKind::Timer
    );

    html! {
        @if show_header {
            div class={ "widget-shell widget-" (kind.as_str()) } data-item-id=(item_id) {
                div.widget-header {
                    span.widget-header-title { (icon) span { (title) } }
                }
                div.widget-body { (body) }
            }
        } @else {
            div class={ "widget-shell widget-shell-flush widget-" (kind.as_str()) } data-item-id=(item_id) {
                (body)
            }
        }
    }
}

fn render_app(options: &Value, data: &Value) -> Markup {
    let Some(app) = data.get("app").filter(|v| !v.is_null()) else {
        return empty_box("Select an App");
    };
    let name = app.get("name").and_then(|v| v.as_str()).unwrap_or("App");
    let desc = app
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let icon = safe_url(
        app.get("iconUrl")
            .and_then(|v| v.as_str())
            .unwrap_or("/logo/homarr.svg"),
    );
    let href = safe_url(app.get("href").and_then(|v| v.as_str()).unwrap_or("#"));
    // pingUrl is serialized as null when unset; fall back to href like upstream.
    let non_empty = |key: &str| {
        app.get(key)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
    };
    let ping_url = non_empty("pingUrl")
        .or_else(|| non_empty("href"))
        .unwrap_or("");
    let show_title = options
        .get("showTitle")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let ping_enabled = options
        .get("pingEnabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let ping_cls = (ping_enabled && !ping_url.is_empty()).then(|| {
        match data
            .get("ping")
            .and_then(|p| p.get("statusCode"))
            .and_then(|s| s.as_u64())
        {
            Some(code) if code < 400 => "ping-dot up",
            Some(_) => "ping-dot down",
            None => "ping-dot unknown",
        }
    });

    let title_attr = if desc.is_empty() {
        name.to_string()
    } else {
        format!("{name} — {desc}")
    };
    let app_id = app.get("id").and_then(|v| v.as_str()).unwrap_or("");

    html! {
        a.app-tile href=(href) target="_blank" rel="noopener noreferrer" data-ping-url=(ping_url) data-app-id=(app_id) title=(title_attr) {
            img.app-icon src=(icon) alt=(name) loading="lazy" data-fallback-src="/logo/homarr.svg";
            @if show_title {
                div.app-name { (name) }
            }
            @if let Some(cls) = ping_cls {
                span class=(cls) title="Service status" {}
            }
        }
    }
}

fn render_bookmarks(options: &Value) -> Markup {
    let items = options
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if items.is_empty() {
        return empty_box("No bookmarks configured");
    }
    html! {
        div.bookmark-chips {
            @for item in items.iter().take(20) {
                @let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("Link");
                @let href = safe_url(item.get("href").and_then(|v| v.as_str()).unwrap_or("#"));
                @let icon_url = item
                    .get("iconUrl")
                    .and_then(|v| v.as_str())
                    .map(safe_url)
                    .filter(|u| *u != "#");
                a.bookmark-chip href=(href) target="_blank" rel="noopener noreferrer" {
                    @if let Some(url) = icon_url {
                        img.bookmark-chip-icon src=(url) alt="" loading="lazy" data-hide-on-error;
                    } @else {
                        // safe: static SVG const.
                        (PreEscaped(r#"<svg class="ui-icon" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M9 15l6 -6" /><path d="M11 6l.463 -.536a5 5 0 0 1 7.071 7.072l-.534 .464" /><path d="M13 18l-.397 .534a5.068 5.068 0 0 1 -7.127 0a4.972 4.972 0 0 1 0 -7.071l.524 -.463" /></svg>"#))
                    }
                    span.bookmark-chip-name { (name) }
                    // safe: static SVG const.
                    (PreEscaped(r#"<svg class="bookmark-chip-ext" viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M12 6h-6a2 2 0 0 0 -2 2v10a2 2 0 0 0 2 2h10a2 2 0 0 0 2 -2v-6" /><path d="M11 13l9 -9" /><path d="M15 4h5v5" /></svg>"#))
                }
            }
        }
    }
}

fn render_clock(options: &Value, data: &Value) -> Markup {
    let is_24 = options
        .get("is24HourFormat")
        .or_else(|| options.get("is24Hour"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let time = data
        .get(if is_24 { "time24" } else { "time12" })
        .and_then(|v| v.as_str())
        .unwrap_or("--:--");
    let show_date = options
        .get("showDate")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let date = if show_date {
        data.get("date").and_then(|v| v.as_str()).unwrap_or("")
    } else {
        ""
    };
    let title = options
        .get("customTitle")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    html! {
        div.clock-widget data-24h=(is_24) {
            @if !title.trim().is_empty() {
                div.clock-sub { (title) }
            }
            div.clock-time { (time) }
            @if !date.is_empty() {
                div.clock-date { (date) }
            }
        }
    }
}

fn render_weather(options: &Value, data: &Value) -> Markup {
    if let Some(err) = data.get("error").and_then(|v| v.as_str()) {
        return empty_box(err);
    }
    let city = data
        .get("locationName")
        .and_then(|v| v.as_str())
        .unwrap_or("Weather");
    let current = data.get("weather").and_then(|w| w.get("current"));
    let temp = current
        .and_then(|c| c.get("temperature2m"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let wind = current
        .and_then(|c| c.get("windSpeed10m"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let code = current
        .and_then(|c| c.get("weatherCode"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let is_day = current
        .and_then(|c| c.get("isDay"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let is_f = options
        .get("isFormatFahrenheit")
        .or_else(|| options.get("isFahrenheit"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let convert_temp = |c: f64| if is_f { c * 9.0 / 5.0 + 32.0 } else { c };
    let unit = if is_f { "°F" } else { "°C" };
    let display_temp = format!("{:.1}{unit}", convert_temp(temp));
    let (label, svg_icon) = weather_visual(code, is_day);

    let hi_lo = data
        .get("weather")
        .and_then(|w| w.get("daily"))
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
        .and_then(|day| {
            let max_c = day.get("temperature2mMax").and_then(|v| v.as_f64())?;
            let min_c = day.get("temperature2mMin").and_then(|v| v.as_f64())?;
            Some(format!(
                "H {:.0}° / L {:.0}°",
                convert_temp(max_c),
                convert_temp(min_c)
            ))
        });

    html! {
        div.weather-widget {
            div.weather-top {
                div.weather-temp-group {
                    div.weather-temp { (display_temp) }
                    div.weather-cond { (label) }
                }
                // safe: static SVG const.
                div.weather-icon-wrap { (PreEscaped(svg_icon)) }
            }
            div.weather-meta {
                span.weather-city { (city) }
                span.weather-pills {
                    @if let Some(hl) = hi_lo {
                        span { (hl) }
                    }
                    span { "Wind " (format!("{wind:.0}")) " km/h" }
                }
            }
        }
    }
}

fn render_air_quality(data: &Value) -> Markup {
    let aq = data.get("airQuality");
    let eu = aq
        .and_then(|v| v.get("europeanAqi"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let pm25 = aq
        .and_then(|v| v.get("pm25").or_else(|| v.get("pm2_5")))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let quality = if eu <= 20.0 {
        "Good"
    } else if eu <= 40.0 {
        "Fair"
    } else if eu <= 60.0 {
        "Moderate"
    } else {
        "Poor"
    };
    html! {
        div.stats-grid style="height:100%;align-content:center" {
            div.stat-pill { span.label { "EU AQI (" (quality) ")" } strong { (format!("{eu:.0}")) } }
            div.stat-pill { span.label { "PM2.5" } strong { (format!("{pm25:.1}")) " µg/m³" } }
        }
    }
}

/// Escapes `text` and applies the inline `**bold**` / `` `code` `` markdown.
fn inline_md(text: &str) -> Markup {
    let escaped = html! { (text) }.into_string();
    // safe: input is Maud-escaped; render_inline_md only adds <strong>/<code> tags.
    PreEscaped(render_inline_md(&escaped))
}

fn render_inline_md(escaped: &str) -> String {
    let mut out = String::with_capacity(escaped.len());
    let mut rest = escaped;
    while !rest.is_empty() {
        let next_bold = rest.find("**");
        let next_code = rest.find('`');
        let pick_bold = match (next_bold, next_code) {
            (Some(b), Some(c)) => Some(b <= c),
            (Some(_), None) => Some(true),
            (None, Some(_)) => Some(false),
            (None, None) => None,
        };
        match pick_bold {
            Some(true) => {
                let b = next_bold.unwrap_or(0);
                out.push_str(&rest[..b]);
                let after = &rest[b + 2..];
                if let Some(end) = after.find("**") {
                    out.push_str("<strong>");
                    out.push_str(&after[..end]);
                    out.push_str("</strong>");
                    rest = &after[end + 2..];
                } else {
                    out.push_str("**");
                    rest = after;
                }
            }
            Some(false) => {
                let c = next_code.unwrap_or(0);
                out.push_str(&rest[..c]);
                let after = &rest[c + 1..];
                if let Some(end) = after.find('`') {
                    out.push_str("<code>");
                    out.push_str(&after[..end]);
                    out.push_str("</code>");
                    rest = &after[end + 1..];
                } else {
                    out.push('`');
                    rest = after;
                }
            }
            None => {
                out.push_str(rest);
                break;
            }
        }
    }
    out
}

/// Finds the `>` that ends a tag, skipping quoted attribute values.
fn tag_end(s: &str) -> Option<usize> {
    let mut quote = None;
    for (i, c) in s.char_indices() {
        match (quote, c) {
            (None, '>') => return Some(i),
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), _) if q == c => quote = None,
            _ => {}
        }
    }
    None
}

/// Value of the first attribute named `wanted` in the attribute part of a tag.
fn attr_value<'a>(attrs: &'a str, wanted: &str) -> Option<&'a str> {
    let mut s = attrs;
    loop {
        s = s.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == '/');
        if s.is_empty() {
            return None;
        }
        let name_end = s
            .find(|c: char| c.is_ascii_whitespace() || c == '=' || c == '/')
            .unwrap_or(s.len());
        let name = &s[..name_end];
        s = s[name_end..].trim_start();
        let mut value = None;
        if let Some(v) = s.strip_prefix('=') {
            let v = v.trim_start();
            let (val, next) = match v.chars().next() {
                Some(q @ ('"' | '\'')) => {
                    let body = &v[1..];
                    let e = body.find(q).unwrap_or(body.len());
                    (&body[..e], body.get(e + 1..).unwrap_or(""))
                }
                _ => {
                    let e = v.find(|c: char| c.is_ascii_whitespace()).unwrap_or(v.len());
                    (&v[..e], &v[e..])
                }
            };
            value = Some(val);
            s = next;
        }
        if name.eq_ignore_ascii_case(wanted) {
            return value;
        }
    }
}

/// Allowlist sanitizer for notebook HTML (the original stores Tiptap HTML). Keeps basic
/// formatting tags without attributes, except `a[href]` with an http(s)/mailto URL; drops
/// script-like elements with their content and every other tag (keeping its text).
fn sanitize_notebook_html(input: &str) -> String {
    const ALLOWED: &str = "p br strong em u s h1 h2 h3 h4 h5 h6 ul ol li blockquote code pre a hr \
                           table thead tbody tr th td";
    const DROP_WITH_CONTENT: &str =
        "script style iframe object embed template noscript textarea title svg math";
    let mut out = String::with_capacity(input.len());
    let mut open: Vec<&str> = Vec::new();
    let mut rest = input;
    while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt].replace('>', "&gt;"));
        let after = &rest[lt + 1..];
        if let Some(comment) = after.strip_prefix("!--") {
            rest = comment.find("-->").map_or("", |i| &comment[i + 3..]);
            continue;
        }
        let closing = after.starts_with('/');
        let name_src = if closing { &after[1..] } else { after };
        if !name_src.starts_with(|c: char| c.is_ascii_alphabetic()) {
            if closing || after.starts_with(['!', '?']) {
                // Doctype, processing instruction or bogus comment.
                rest = after.find('>').map_or("", |i| &after[i + 1..]);
            } else {
                out.push_str("&lt;");
                rest = after;
            }
            continue;
        }
        let name_len = name_src
            .find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(name_src.len());
        let name = name_src[..name_len].to_ascii_lowercase();
        let Some(end) = tag_end(&name_src[name_len..]) else {
            // Unterminated tag: drop the remainder.
            rest = "";
            break;
        };
        let attrs = &name_src[name_len..name_len + end];
        rest = &name_src[name_len + end + 1..];
        let allowed = ALLOWED.split_whitespace().find(|t| *t == name);
        if closing {
            if let Some(pos) = allowed.and_then(|tag| open.iter().rposition(|t| *t == tag)) {
                for t in open.drain(pos..).rev() {
                    out.push_str("</");
                    out.push_str(t);
                    out.push('>');
                }
            }
            continue;
        }
        if DROP_WITH_CONTENT.split(' ').any(|t| t == name) {
            let lower = rest.to_ascii_lowercase();
            rest = match lower.find(&format!("</{name}")) {
                Some(i) => rest[i..].find('>').map_or("", |j| &rest[i + j + 1..]),
                None => "",
            };
            continue;
        }
        let Some(tag) = allowed else {
            continue;
        };
        out.push('<');
        out.push_str(tag);
        if tag == "a" {
            if let Some(href) = attr_value(attrs, "href").map(str::trim) {
                let lower = href.to_ascii_lowercase();
                if ["http://", "https://", "mailto:"]
                    .iter()
                    .any(|p| lower.starts_with(p))
                {
                    // `&` stays as written: entities like `&amp;` are valid inside attributes.
                    let href = href
                        .replace('"', "&quot;")
                        .replace('<', "&lt;")
                        .replace('>', "&gt;");
                    out.push_str(&format!(
                        r#" href="{href}" target="_blank" rel="noopener noreferrer""#
                    ));
                }
            }
        }
        out.push('>');
        if tag != "br" && tag != "hr" {
            open.push(tag);
        }
    }
    out.push_str(&rest.replace('>', "&gt;"));
    for t in open.into_iter().rev() {
        out.push_str("</");
        out.push_str(t);
        out.push('>');
    }
    out
}

enum NoteBlock<'a> {
    Heading(u8, &'a str),
    Para(&'a str),
    /// List items; `Some(done)` marks a task item.
    List(Vec<(Option<bool>, &'a str)>),
}

fn render_notebook(options: &Value) -> Markup {
    let content = options
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if content.trim().is_empty() {
        return empty_box("Empty note");
    }
    // Content saved by the original (Tiptap) is HTML; older port content is markdown-like text.
    if content.trim_start().starts_with('<') {
        // safe: output of the allowlist sanitizer above.
        return html! { div.notebook-body { (PreEscaped(sanitize_notebook_html(content))) } };
    }
    let mut blocks: Vec<NoteBlock> = Vec::new();
    let mut in_list = false;
    for line in content.lines().map(str::trim) {
        let item = if let Some(t) = line
            .strip_prefix("- [x] ")
            .or_else(|| line.strip_prefix("- [X] "))
        {
            Some((Some(true), t))
        } else if let Some(t) = line.strip_prefix("- [ ] ") {
            Some((Some(false), t))
        } else {
            line.strip_prefix("- ")
                .or_else(|| line.strip_prefix("* "))
                .map(|t| (None, t))
        };
        match item {
            Some(it) => {
                if !in_list {
                    blocks.push(NoteBlock::List(Vec::new()));
                    in_list = true;
                }
                if let Some(NoteBlock::List(items)) = blocks.last_mut() {
                    items.push(it);
                }
            }
            None => {
                in_list = false;
                if line.is_empty() {
                    continue;
                }
                blocks.push(if let Some(h) = line.strip_prefix("### ") {
                    NoteBlock::Heading(6, h)
                } else if let Some(h) = line.strip_prefix("## ") {
                    NoteBlock::Heading(5, h)
                } else if let Some(h) = line.strip_prefix("# ") {
                    NoteBlock::Heading(4, h)
                } else {
                    NoteBlock::Para(line)
                });
            }
        }
    }
    html! {
        div.notebook-body {
            @for block in &blocks {
                @match block {
                    NoteBlock::Heading(4, t) => h4 { (inline_md(t)) },
                    NoteBlock::Heading(5, t) => h5 { (inline_md(t)) },
                    NoteBlock::Heading(_, t) => h6 { (inline_md(t)) },
                    NoteBlock::Para(t) => p { (inline_md(t)) },
                    NoteBlock::List(items) => ul.notebook-list {
                        @for (task, t) in items {
                            @if let Some(done) = task {
                                li.notebook-task {
                                    input type="checkbox" disabled checked[*done];
                                    span { (inline_md(t)) }
                                }
                            } @else {
                                li { (inline_md(t)) }
                            }
                        }
                    },
                }
            }
        }
    }
}

fn render_countdown(options: &Value) -> Markup {
    let title = options
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Countdown");
    let date = options.get("date").and_then(|v| v.as_str()).unwrap_or("");
    html! {
        div.countdown-widget data-target-date=(date) {
            div.metric-sub { (title) }
            div.metric-big.js-countdown { "--d --h --m" }
            div.metric-sub { (date) }
        }
    }
}

fn render_timer(options: &Value) -> Markup {
    let title = options
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Timer");
    let secs = options
        .get("durationSeconds")
        .and_then(|v| v.as_u64())
        .unwrap_or(1500);
    html! {
        div.timer-widget data-duration=(secs) data-remaining=(secs) {
            div.metric-sub { (title) }
            div.metric-big.js-timer-display { (format!("{:02}:{:02}", secs / 60, secs % 60)) }
            div.timer-controls {
                button.btn.btn-sm type="button" data-action="toggle-timer" { "Start" }
                button.btn.btn-sm type="button" data-action="reset-timer" { "Reset" }
            }
        }
    }
}

fn render_iframe(options: &Value, self_origin: Option<&str>) -> Markup {
    let raw = options
        .get("embedUrl")
        .and_then(|v| v.as_str())
        .unwrap_or("about:blank");
    let safe = safe_url(raw);
    if safe == "#" {
        return empty_box("Invalid embed URL");
    }
    // Same-origin (relative or matching Homarr's own origin) targets must never get
    // `allow-same-origin`, otherwise the framed page could reach Homarr's origin.
    let sandbox = if safe.starts_with('/') || is_same_origin_url(safe, self_origin) {
        "allow-scripts allow-forms"
    } else {
        "allow-scripts allow-same-origin allow-forms"
    };
    html! { iframe.widget-iframe src=(safe) sandbox=(sandbox) loading="lazy" {} }
}

fn render_video(options: &Value) -> Markup {
    let raw = options
        .get("feedUrl")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let safe = safe_url(raw);
    if safe == "#" {
        return empty_box("No video stream configured");
    }
    html! { video.widget-video src=(safe) controls muted playsinline {} }
}

fn render_rss(data: &Value) -> Markup {
    let entries = data
        .get("entries")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if entries.is_empty() {
        return empty_box("No RSS items available");
    }
    html! {
        ul.compact-list {
            @for e in entries.iter().take(10) {
                li {
                    a href=(safe_url(e.get("link").and_then(|v| v.as_str()).unwrap_or("#")))
                        target="_blank" rel="noopener noreferrer" {
                        (e.get("title").and_then(|v| v.as_str()).unwrap_or("Untitled"))
                    }
                }
            }
        }
    }
}

fn render_stock(data: &Value) -> Markup {
    let Some(stock) = data.get("stock") else {
        return empty_box("Stock quote unavailable");
    };
    let sym = stock.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
    let price = stock
        .get("regularMarketPrice")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let pct = stock
        .get("changePercent")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let cls = if pct >= 0.0 { "up" } else { "down" };
    html! {
        div.stock-widget {
            strong { (sym) }
            " "
            span.metric-big { (format!("${price:.2}")) }
            " "
            span.badge.(cls) { (format!("{pct:+.2}%")) }
        }
    }
}

fn render_minecraft(data: &Value) -> Markup {
    let Some(st) = data.get("status") else {
        return empty_box("No server status");
    };
    let online = st.get("online").and_then(|v| v.as_bool()).unwrap_or(false);
    let host = st.get("host").and_then(|v| v.as_str()).unwrap_or("");
    let cur = st
        .get("playersOnline")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let max = st.get("playersMax").and_then(|v| v.as_u64()).unwrap_or(0);
    html! {
        div {
            @if online {
                span.badge.up { "Online" }
            } @else {
                span.badge.down { "Offline" }
            }
            " "
            strong { (host) }
            div.metric-sub { "Players: " (cur) " / " (max) }
        }
    }
}

fn render_release(data: &Value) -> Markup {
    let Some(rel) = data.get("release") else {
        return empty_box("Release data unavailable");
    };
    let id = rel.get("identifier").and_then(|v| v.as_str()).unwrap_or("");
    let tag = rel
        .get("latestRelease")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let url = safe_url(rel.get("url").and_then(|v| v.as_str()).unwrap_or("#"));
    html! {
        div {
            strong { (id) }
            div.metric-big { a href=(url) target="_blank" rel="noopener noreferrer" { (tag) } }
        }
    }
}

fn render_docker(data: &Value) -> Markup {
    let items = data
        .get("containers")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if items.is_empty() {
        return empty_box("No Docker containers reported");
    }
    html! {
        ul.compact-list {
            @for c in items.iter().take(12) {
                @let state = c.get("state").and_then(|v| v.as_str()).unwrap_or("");
                li {
                    span { (c.get("name").and_then(|v| v.as_str()).unwrap_or("")) }
                    " "
                    span.badge.(if state == "running" { "up" } else { "down" }) { (state) }
                }
            }
        }
    }
}

fn render_downloads(data: &Value) -> Markup {
    let Some(q) = data.get("queue") else {
        return empty_box("Queue unavailable");
    };
    let items = q
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let dl = q
        .get("downloadSpeedBps")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let ul = q
        .get("uploadSpeedBps")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    html! {
        div.downloads-summary {
            span.speed-pill.speed-dl { "↓ " (format_speed(dl)) }
            span.speed-pill.speed-ul { "↑ " (format_speed(ul)) }
            span.muted style="margin-left:auto;font-size:.76rem" { (items.len()) " active" }
        }
        div.downloads-list {
            @for item in items.iter().take(6) {
                @let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
                @let state = item.get("state").and_then(|v| v.as_str()).unwrap_or("");
                @let pct = (item.get("progress").and_then(|v| v.as_f64()).unwrap_or(0.0) * 100.0)
                    .clamp(0.0, 100.0);
                @let size = item
                    .get("sizeBytes")
                    .and_then(|v| v.as_u64())
                    .map(format_si_bytes)
                    .unwrap_or_default();
                @let eta = item
                    .get("etaSeconds")
                    .and_then(|v| v.as_u64())
                    .map(|e| format!(" · ETA {}", format_eta(e)))
                    .unwrap_or_default();
                div.download-row {
                    div.download-top {
                        span.download-name title=(name) { (name) }
                        span.badge { (state) }
                    }
                    div.progress-track {
                        div.progress-fill.is-green[state == "seeding"] style=(format!("width:{pct:.1}%")) {}
                    }
                    div.download-meta {
                        span { (size) (eta) }
                        span { (format!("{pct:.0}%")) }
                    }
                }
            }
        }
    }
}

fn render_media_sessions(data: &Value) -> Markup {
    let sessions = data
        .get("sessions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if sessions.is_empty() {
        return empty_box("Nothing playing right now");
    }
    html! {
        div.media-sessions {
            @for s in sessions.iter().take(6) {
                @let item = s.get("itemName").and_then(|v| v.as_str()).unwrap_or("");
                @let series = s
                    .get("seriesName")
                    .and_then(|v| v.as_str())
                    .filter(|v| !v.trim().is_empty());
                @let user = s.get("username").and_then(|v| v.as_str()).unwrap_or("");
                @let client = s
                    .get("clientName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Player");
                @let is_paused = s.get("isPaused").and_then(|v| v.as_bool()).unwrap_or(false);
                @let pct = s
                    .get("progressPercent")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0)
                    .clamp(0.0, 100.0);
                div.media-session-card {
                    div.media-session-top {
                        div {
                            @if let Some(sr) = series {
                                div.media-title { (sr) }
                                div.media-sub { (item) }
                            } @else {
                                div.media-title { (item) }
                            }
                        }
                        @if is_paused {
                            span.badge.muted { "Paused" }
                        } @else {
                            span.badge.up { "Playing" }
                        }
                    }
                    div.progress-track {
                        div.progress-fill style=(format!("width:{pct:.1}%")) {}
                    }
                    div.media-meta {
                        span { (user) " · " (client) }
                        span { (format!("{pct:.0}%")) }
                    }
                }
            }
        }
    }
}

fn stat_pill(label: &str, value: impl maud::Render) -> Markup {
    html! { div.stat-pill { span.label { (label) } strong { (value) } } }
}

fn render_media_requests(kind: WidgetKind, data: &Value) -> Markup {
    if kind == WidgetKind::MediaRequestsRequestStats {
        let s = data.get("stats");
        let count = |key: &str| {
            s.and_then(|v| v.get(key))
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
        };
        return html! {
            div.stats-grid {
                (stat_pill("Total", count("total")))
                (stat_pill("Pending", count("pending")))
                (stat_pill("Available", count("available")))
            }
        };
    }
    let items = data
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    html! {
        ul.compact-list {
            @for item in items.iter().take(8) {
                li {
                    span { (item.get("title").and_then(|v| v.as_str()).unwrap_or("")) }
                    " "
                    span.badge { (item.get("status").and_then(|v| v.as_str()).unwrap_or("")) }
                }
            }
        }
    }
}

fn render_calendar(data: &Value) -> Markup {
    let events = data
        .get("events")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if events.is_empty() {
        return empty_box("No upcoming releases");
    }
    html! {
        div.calendar-list {
            @for ev in events.iter().take(8) {
                @let start = ev
                    .get("startDate")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .get(..10)
                    .unwrap_or("");
                @let color = safe_hex_color(
                    ev.get("indicatorColor")
                        .and_then(|v| v.as_str())
                        .unwrap_or("#339af0"),
                );
                div.calendar-item {
                    span.calendar-bar style={ "background:" (color) } {}
                    div.calendar-info {
                        div.calendar-title { (ev.get("title").and_then(|v| v.as_str()).unwrap_or("")) }
                        div.calendar-sub { (ev.get("subTitle").and_then(|v| v.as_str()).unwrap_or("")) }
                    }
                    span.badge { (start) }
                }
            }
        }
    }
}

fn dns_tile(cls: &str, svg: &'static str, value: &str, label: &str) -> Markup {
    html! {
        div class={ "dns-tile " (cls) } {
            // safe: static SVG const.
            (PreEscaped(svg))
            div.dns-tile-value { (value) }
            div.dns-tile-label { (label) }
        }
    }
}

fn render_dns_hole(kind: WidgetKind, data: &Value) -> Markup {
    let first = data
        .get("items")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|x| x.get("summary"));
    let Some(s) = first else {
        return empty_box("DNS Hole metrics unavailable");
    };
    let count = |key: &str| s.get(key).and_then(|v| v.as_u64()).unwrap_or(0);
    let pct = s
        .get("adsPercentageToday")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let status = s
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("enabled");

    html! {
        @if kind == WidgetKind::DnsHoleControls {
            div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:8px" {
                span.metric-sub { "Pi-hole / AdGuard" }
                span.badge.up { (status) }
            }
        }
        div.dns-hole-grid {
            (dns_tile("dns-red", r#"<svg class="dns-tile-icon" viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M13.252 20.601c-.408 .155 -.826 .288 -1.252 .399a12 12 0 0 1 -8.5 -15a12 12 0 0 0 8.5 -3a12 12 0 0 0 8.5 3a12 12 0 0 1 -.19 7.357" /><path d="M22 22l-5 -5" /><path d="M17 22l5 -5" /></svg>"#,
                &format_compact_count(count("adsBlockedToday")), "Ads Blocked Today"))
            (dns_tile("dns-yellow", r#"<svg class="dns-tile-icon" viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M17 17m-1 0a1 1 0 1 0 2 0a1 1 0 1 0 -2 0" /><path d="M7 7m-1 0a1 1 0 1 0 2 0a1 1 0 1 0 -2 0" /><path d="M6 18l12 -12" /></svg>"#,
                &format!("{pct:.1}%"), "Queries Blocked"))
            (dns_tile("dns-cyan", r#"<svg class="dns-tile-icon" viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M10 10m-7 0a7 7 0 1 0 14 0a7 7 0 1 0 -14 0" /><path d="M21 21l-6 -6" /></svg>"#,
                &format_compact_count(count("dnsQueriesToday")), "DNS Queries Today"))
            (dns_tile("dns-green", r#"<svg class="dns-tile-icon" viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12a9 9 0 1 0 18 0a9 9 0 0 0 -18 0" /><path d="M3.6 9h16.8" /><path d="M3.6 15h16.8" /><path d="M11.5 3a17 17 0 0 0 0 18" /><path d="M12.5 3a17 17 0 0 1 0 18" /></svg>"#,
                &format_compact_count(count("domainsBeingBlocked")), "Domains on Blocklist"))
        }
    }
}

fn render_donut_gauge(label: &str, pct: f64, sub: &str, color_var: &str) -> Markup {
    let clamped = pct.clamp(0.0, 100.0);
    let circ = 150.796_f64;
    let offset = circ * (1.0 - clamped / 100.0);
    html! {
        div.health-gauge {
            div.gauge-ring {
                svg viewBox="0 0 64 64" width="60" height="60" aria-hidden="true" {
                    circle.gauge-track cx="32" cy="32" r="24" {}
                    circle.gauge-arc cx="32" cy="32" r="24"
                        style=(format!("stroke:{color_var};stroke-dasharray:{circ:.1};stroke-dashoffset:{offset:.1}")) {}
                }
                span.gauge-pct { (format!("{clamped:.0}%")) }
            }
            div.gauge-text {
                div.gauge-label { (label) }
                div.gauge-sub { (sub) }
            }
        }
    }
}

fn render_health(_kind: WidgetKind, data: &Value) -> Markup {
    let Some(h) = data.get("health") else {
        return empty_box("Health telemetry unavailable");
    };
    let name = h
        .get("systemName")
        .and_then(|v| v.as_str())
        .unwrap_or("Host");
    let uptime = h
        .get("uptimeSeconds")
        .and_then(|v| v.as_u64())
        .map(format_uptime)
        .unwrap_or_else(|| "-".to_string());
    let cpu = h.get("cpuPercent").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let cpu_sub = h
        .get("cpuTempC")
        .and_then(|v| v.as_f64())
        .map(|t| format!("{t:.0}°C"))
        .unwrap_or_else(|| "Load avg".to_string());
    let mem_used = h
        .get("memoryUsedBytes")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let mem_total = h
        .get("memoryTotalBytes")
        .and_then(|v| v.as_u64())
        .unwrap_or(1)
        .max(1);
    let mem_pct = (mem_used as f64 / mem_total as f64) * 100.0;
    let mem_sub = format!("{} / {} GiB", format_gib(mem_used), format_gib(mem_total));
    let disks = h
        .get("disks")
        .and_then(|v| v.as_array())
        .filter(|d| !d.is_empty());
    let nodes = h
        .get("nodes")
        .and_then(|v| v.as_array())
        .filter(|n| !n.is_empty());

    html! {
        div.health-host-row {
            strong { (name) }
            span.badge.up { "Up " (uptime) }
        }
        div.health-gauges {
            (render_donut_gauge("CPU", cpu, &cpu_sub, "var(--mantine-blue)"))
            (render_donut_gauge("Memory", mem_pct, &mem_sub, "var(--mantine-violet)"))
        }
        @if let Some(disks) = disks {
            div.health-disks {
                @for d in disks.iter().take(2) {
                    @let mount = d
                        .get("mountPoint")
                        .or_else(|| d.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("/");
                    @let used = d.get("usedBytes").and_then(|v| v.as_u64()).unwrap_or(0);
                    @let total = d
                        .get("totalBytes")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(1)
                        .max(1);
                    @let pct = (used as f64 / total as f64 * 100.0).clamp(0.0, 100.0);
                    div.health-disk-row {
                        div.download-meta {
                            span { code { (mount) } }
                            span { (format!("{pct:.0}%")) }
                        }
                        div.progress-track {
                            div.progress-fill.is-cyan style=(format!("width:{pct:.1}%")) {}
                        }
                    }
                }
            }
        }
        @if let Some(nodes) = nodes {
            div.node-pills {
                @for n in nodes.iter().take(4) {
                    @let online = n.get("status").and_then(|v| v.as_str()).unwrap_or("online") == "online";
                    span.node-pill {
                        span.dot.(if online { "up" } else { "down" }) {}
                        (n.get("name").and_then(|v| v.as_str()).unwrap_or(""))
                    }
                }
            }
        }
    }
}

fn render_smart_home(data: &Value) -> Markup {
    let entities = data
        .get("entities")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if entities.is_empty() {
        return empty_box("No smart home entities found");
    }
    html! {
        div.smart-home-grid {
            @for e in entities.iter().take(6) {
                @let name = e
                    .get("friendlyName")
                    .or_else(|| e.get("entityId"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                @let st = e.get("state").and_then(|v| v.as_str()).unwrap_or("");
                @let unit = e
                    .get("unitOfMeasurement")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                @let state_cls = if st.eq_ignore_ascii_case("on") {
                    "badge up"
                } else if st.eq_ignore_ascii_case("off") {
                    "badge muted"
                } else {
                    "badge"
                };
                div.smart-entity-tile {
                    span.smart-entity-name { (name) }
                    span class=(state_cls) { (st) (unit) }
                }
            }
        }
    }
}

fn render_notifications(data: &Value) -> Markup {
    let items = data
        .get("notifications")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if items.is_empty() {
        return empty_box("No recent notifications");
    }
    html! {
        ul.compact-list {
            @for n in items.iter().take(6) {
                li { (n.get("title").and_then(|v| v.as_str()).unwrap_or("")) }
            }
        }
    }
}

fn render_indexers(data: &Value) -> Markup {
    let items = data
        .get("indexers")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if items.is_empty() {
        return empty_box("No indexers");
    }
    html! {
        ul.compact-list {
            @for idx in items.iter().take(8) {
                li {
                    span { (idx.get("name").and_then(|v| v.as_str()).unwrap_or("")) }
                    " "
                    span.badge { (idx.get("status").and_then(|v| v.as_str()).unwrap_or("ok")) }
                }
            }
        }
    }
}

fn render_ups(data: &Value) -> Markup {
    let Some(first) = data
        .get("ups")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
    else {
        return empty_box("No UPS telemetry");
    };
    let name = first.get("name").and_then(|v| v.as_str()).unwrap_or("UPS");
    let charge = first
        .get("batteryChargePercent")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let load = first
        .get("loadPercent")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    html! {
        div.stats-grid {
            (stat_pill(&format!("{name} Battery"), format!("{charge:.0}%")))
            (stat_pill("Load", format!("{load:.0}%")))
        }
    }
}

fn render_stats(data: &Value) -> Markup {
    let metrics = data
        .get("stats")
        .and_then(|s| s.get("metrics"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if metrics.is_empty() {
        return empty_box("No stats available");
    }
    let field = |m: &Value, key: &str| m.get(key).and_then(|v| v.as_str()).unwrap_or("").to_owned();
    html! {
        div.stats-grid {
            @for m in metrics.iter().take(6) {
                (stat_pill(&field(m, "label"), field(m, "value")))
            }
        }
    }
}

fn render_custom_api(data: &Value) -> Markup {
    if let Some(rendered) = data.get("renderedHtml").and_then(|v| v.as_str()) {
        // safe: template placeholders were already HTML-escaped by `render_template`.
        return html! { div.custom-widget-content { (PreEscaped(rendered)) } };
    }
    empty_box("Configure a Custom API URL or Custom Widget v2 definition")
}

fn render_assistant(options: &Value) -> Markup {
    let prompt = options
        .get("placeholder")
        .and_then(|v| v.as_str())
        .unwrap_or("Ask Homarr Assistant...");
    html! {
        div.assistant-cta {
            p.metric-sub { (prompt) }
            a.btn.btn-sm href="/manage/assistant" { "Open Assistant" }
        }
    }
}

fn render_stats_with_extras(kind: WidgetKind, data: &Value) -> Markup {
    let alerts = data
        .get("alerts")
        .and_then(|v| v.as_array())
        .filter(|a| kind == WidgetKind::BeszelAlerts && !a.is_empty());
    html! {
        (render_stats(data))
        @if let Some(alerts) = alerts {
            ul.compact-list style="margin-top:8px" {
                @for alert in alerts.iter().take(4) {
                    @let name = alert
                        .get("name")
                        .or_else(|| alert.get("id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("Alert");
                    @let level = alert
                        .get("value")
                        .and_then(|v| v.as_f64())
                        .map(|n| format!("{n:.0}"))
                        .unwrap_or_else(|| "active".to_string());
                    li { span { (name) } " " span.badge { (level) } }
                }
            }
        }
    }
}

fn render_error_box(data: &Value) -> Option<Markup> {
    data.get("error").and_then(|v| v.as_str()).map(empty_box)
}

fn render_media_releases(data: &Value) -> Markup {
    if let Some(err_html) = render_error_box(data) {
        return err_html;
    }
    let releases = data
        .get("releases")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if releases.is_empty() {
        return empty_box("No recent media releases");
    }
    html! {
        ul.compact-list {
            @for rel in releases.iter().take(6) {
                @let title = rel
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Untitled");
                @let sub = rel
                    .get("subtitle")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty());
                @let href = safe_url(rel.get("href").and_then(|v| v.as_str()).unwrap_or("#"));
                @let date = rel
                    .get("releaseDate")
                    .and_then(|v| v.as_str())
                    .map(|s| s.chars().take(10).collect::<String>())
                    .unwrap_or_default();
                @let label = html! {
                    (title)
                    @if let Some(s) = sub {
                        " "
                        span.muted { (s) }
                    }
                };
                li {
                    @if href == "#" {
                        span { (label) }
                    } @else {
                        a href=(href) target="_blank" rel="noopener noreferrer" { (label) }
                    }
                    " "
                    span.badge { (date) }
                }
            }
        }
    }
}

fn render_media_missing(data: &Value) -> Markup {
    if let Some(err_html) = render_error_box(data) {
        return err_html;
    }
    let items = data
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if items.is_empty() {
        return empty_box("No missing media");
    }
    html! {
        ul.compact-list {
            @for item in items.iter().take(8) {
                li {
                    span { (item.get("title").and_then(|v| v.as_str()).unwrap_or("")) }
                    " "
                    span.badge { (item.get("subtitle").and_then(|v| v.as_str()).unwrap_or("Missing")) }
                }
            }
        }
    }
}

fn render_media_transcoding(data: &Value) -> Markup {
    if let Some(err_html) = render_error_box(data) {
        return err_html;
    }
    let Some(summary) = data.get("summary") else {
        return empty_box("Transcoding stats unavailable");
    };
    let count = |key: &str| summary.get(key).and_then(|v| v.as_u64()).unwrap_or(0);
    let saved = summary
        .get("spaceSavedBytes")
        .and_then(|v| v.as_u64())
        .map(format_si_bytes)
        .unwrap_or_else(|| "0 B".to_string());
    html! {
        div.stats-grid {
            (stat_pill("Files", count("totalFiles")))
            (stat_pill("Queue", count("queueCount")))
            (stat_pill("Workers", count("activeWorkers")))
            (stat_pill("Saved", saved))
        }
    }
}

fn render_firewall(data: &Value) -> Markup {
    if let Some(err_html) = render_error_box(data) {
        return err_html;
    }
    let percent = |group: &str, key: &str| {
        data.get(group)
            .and_then(|c| c.get(key))
            .and_then(|v| v.as_f64())
            .map(|v| format!("{v:.1}%"))
            .unwrap_or_else(|| "—".to_string())
    };
    let ver = data
        .get("version")
        .and_then(|v| v.get("version"))
        .and_then(|v| v.as_str())
        .unwrap_or("OPNsense");
    let if_count = data
        .get("interfaces")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|entry| entry.get("data"))
        .and_then(|d| d.as_array())
        .map(|d| d.len())
        .unwrap_or(0);
    html! {
        div.stats-grid {
            (stat_pill("CPU", percent("cpu", "total")))
            (stat_pill("Memory", percent("memory", "percent")))
            (stat_pill("Interfaces", if_count))
            (stat_pill("Version", ver))
        }
    }
}

fn render_vpn(data: &Value) -> Markup {
    if let Some(err_html) = render_error_box(data) {
        return err_html;
    }
    let Some(vpn) = data.get("vpn") else {
        return empty_box("VPN status unavailable");
    };
    let field = |key: &str, default| vpn.get(key).and_then(|v| v.as_str()).unwrap_or(default);
    html! {
        div.stats-grid {
            (stat_pill("Status", field("vpnStatus", "unknown")))
            (stat_pill("Public IP", field("publicIp", "—")))
            (stat_pill("Country", field("country", "—")))
        }
    }
}

fn render_traefik(data: &Value) -> Markup {
    if let Some(err_html) = render_error_box(data) {
        return err_html;
    }
    let Some(t) = data.get("traefik") else {
        return empty_box("Traefik overview unavailable");
    };
    let sum_field = |group: &str| -> u64 {
        ["http", "tcp", "udp"]
            .iter()
            .filter_map(|p| {
                t.get(p)
                    .and_then(|x| x.get(group))
                    .and_then(|g| g.get("total"))
                    .and_then(|v| v.as_u64())
            })
            .sum()
    };
    let version = t
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("Traefik");
    html! {
        div.stats-grid {
            (stat_pill("Routers", sum_field("routers")))
            (stat_pill("Services", sum_field("services")))
            (stat_pill("Middleware", sum_field("middlewares")))
            (stat_pill("Version", version))
        }
    }
}

fn render_network_controller(data: &Value) -> Markup {
    if let Some(err_html) = render_error_box(data) {
        return err_html;
    }
    let Some(net) = data.get("network") else {
        return empty_box("Network controller unavailable");
    };
    let clients = |key: &str| {
        net.get(key)
            .map(|w| {
                w.get("users").and_then(|v| v.as_u64()).unwrap_or(0)
                    + w.get("guests").and_then(|v| v.as_u64()).unwrap_or(0)
            })
            .unwrap_or(0)
    };
    let latency = net
        .get("www")
        .and_then(|w| w.get("latency"))
        .and_then(|v| v.as_f64())
        .map(|l| format!("{l:.1} ms"))
        .unwrap_or_else(|| "—".to_string());
    let wan = net
        .get("wanStatus")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    html! {
        div.stats-grid {
            (stat_pill("WAN", wan))
            (stat_pill("Wi-Fi", clients("wifi")))
            (stat_pill("LAN", clients("lan")))
            (stat_pill("Latency", latency))
        }
    }
}

fn render_anchor_note(data: &Value) -> Markup {
    if let Some(err_html) = render_error_box(data) {
        return err_html;
    }
    if let Some(note) = data.get("note") {
        return html! {
            div.notebook-body {
                strong { (note.get("title").and_then(|v| v.as_str()).unwrap_or("Note")) }
                p style="margin-top:6px;white-space:pre-wrap" {
                    (note.get("content").and_then(|v| v.as_str()).unwrap_or(""))
                }
            }
        };
    }
    let notes = data
        .get("notes")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if notes.is_empty() {
        return empty_box("No notes found");
    }
    html! {
        ul.compact-list {
            @for note in notes.iter().take(6) {
                li {
                    span { (note.get("title").and_then(|v| v.as_str()).unwrap_or("Untitled")) }
                    " "
                    @if note.get("isPinned").and_then(|v| v.as_bool()).unwrap_or(false) {
                        span.badge { "pinned" }
                    }
                }
            }
        }
    }
}

fn render_generic(data: &Value) -> Markup {
    if let Some(err_html) = render_error_box(data) {
        return err_html;
    }
    let raw = data.get("data").unwrap_or(data);
    let summary = raw.to_string();
    let clipped = if summary.chars().count() > 280 {
        format!("{}...", summary.chars().take(280).collect::<String>())
    } else {
        summary
    };
    html! { pre.generic-json { (clipped) } }
}

#[cfg(test)]
mod notebook_sanitizer_tests {
    use super::sanitize_notebook_html as clean;

    #[test]
    fn removes_scripts_and_event_attributes() {
        assert_eq!(clean("<p>a<script>alert(1)</script>b</p>"), "<p>ab</p>");
        assert_eq!(clean("<SCRIPT>x</SCRIPT >ok"), "ok");
        assert_eq!(clean(r#"<img src=x onerror="alert(1)">t"#), "t");
        assert_eq!(
            clean(r#"<p onclick="x()" style="color:red">t</p>"#),
            "<p>t</p>"
        );
        assert_eq!(clean("<style>p{}</style><svg onload=x><p>a</p></svg>"), "");
        assert_eq!(clean("a < b > c <!-- <script> -->"), "a &lt; b &gt; c ");
        assert_eq!(clean(r#"<p title="a>b">t</p>"#), "<p>t</p>");
        assert_eq!(clean("<p>unterminated <b"), "<p>unterminated </p>");
    }

    #[test]
    fn filters_link_targets() {
        assert_eq!(clean(r#"<a href="javascript:alert(1)">x</a>"#), "<a>x</a>");
        assert_eq!(clean(r#"<a href=" JaVaScRiPt:alert(1)">x</a>"#), "<a>x</a>");
        assert_eq!(clean(r#"<a href="jav&#x61;script:x">x</a>"#), "<a>x</a>");
        assert_eq!(
            clean(r#"<a onclick="x" href='https://e.com/?a=1&amp;b="2"'>x</a>"#),
            r#"<a href="https://e.com/?a=1&amp;b=&quot;2&quot;" target="_blank" rel="noopener noreferrer">x</a>"#
        );
        assert!(clean(r#"<a href="mailto:a@b.c">m</a>"#).contains(r#"href="mailto:a@b.c""#));
    }

    #[test]
    fn keeps_formatting() {
        let html =
            "<h2>T</h2><p><strong>b</strong> <em>i</em> <u>u</u> <s>s</s> <code>c</code></p>\
                    <ul><li>x</li></ul><ol><li>y</li></ol><blockquote>q</blockquote><pre>p</pre>\
                    <hr><br><table><tbody><tr><td>c</td></tr></tbody></table>";
        assert_eq!(clean(html), html);
        assert_eq!(
            clean(r#"<p style="text-align:center"><span style="color:red">r</span></p>"#),
            "<p>r</p>"
        );
        // Unbalanced markup is closed.
        assert_eq!(clean("<ul><li>a</ul>b</p>"), "<ul><li>a</li></ul>b");
        assert_eq!(clean("<p><em>a"), "<p><em>a</em></p>");
    }
}

#[cfg(test)]
mod app_ping_tests {
    use super::render_app;
    use serde_json::json;

    #[test]
    fn null_ping_url_falls_back_to_href() {
        let html = render_app(
            &json!({}),
            &json!({ "app": { "name": "Radarr", "href": "http://10.0.0.5:7878/", "pingUrl": null } }),
        )
        .into_string();
        assert!(html.contains(r#"data-ping-url="http://10.0.0.5:7878/""#));
        assert!(html.contains("ping-dot"));
    }
}
