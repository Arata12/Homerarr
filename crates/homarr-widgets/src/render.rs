use homarr_definitions::WidgetKind;
use maud::{html, Markup, PreEscaped};
use serde_json::Value;
use url::Url;

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
    if data
        .get("missingIntegration")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return html! {
            div class={ "widget-shell widget-shell-flush widget-" (kind.as_str()) } data-item-id=(item_id) {
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
        WidgetKind::MediaServer => render_media_sessions(options, data),
        WidgetKind::MediaRequestsRequestList | WidgetKind::MediaRequestsRequestStats => {
            render_media_requests(kind, options, data)
        }
        WidgetKind::Calendar => render_calendar(data),
        WidgetKind::DnsHoleSummary | WidgetKind::DnsHoleControls => {
            render_dns_hole(kind, options, data)
        }
        WidgetKind::HealthMonitoring => render_health_monitoring(options, data),
        WidgetKind::SystemResources | WidgetKind::SystemDisks => render_health(kind, data),
        WidgetKind::SmartHomeEntityState
        | WidgetKind::SmartHomeExecuteAutomation
        | WidgetKind::SmartHomeControls => render_smart_home(data),
        WidgetKind::Notifications => render_notifications(data),
        WidgetKind::IndexerManager => render_indexers(data),
        WidgetKind::Ups => render_ups(data),
        WidgetKind::MediaMissing => render_media_missing(data),
        WidgetKind::MediaReleases => render_media_releases(options, data),
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
        WidgetKind::UptimeKuma if data.get("pages").is_some() => render_uptime_kuma(options, data),
        WidgetKind::Assistant => render_assistant(options),
        _ if data.get("error").and_then(|v| v.as_str()).is_some() => render_generic(data),
        _ if data.get("stats").is_some() => render_stats_with_extras(kind, data),
        _ => render_generic(data),
    };

    let flush = matches!(
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
    // Like upstream there is no generic title bar; only a user-set `title` option is shown.
    let custom_title = options
        .get("title")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty());

    html! {
        @if flush {
            div class={ "widget-shell widget-shell-flush widget-" (kind.as_str()) } data-item-id=(item_id) {
                (body)
            }
        } @else {
            div class={ "widget-shell widget-" (kind.as_str()) } data-item-id=(item_id) {
                @if let Some(title) = custom_title {
                    div.widget-header {
                        span.widget-header-title { (title) }
                    }
                }
                div.widget-body { (body) }
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
            Some(code) if code < 500 => "ping-dot up",
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
            @if show_title {
                div.app-name { (name) }
            }
            img.app-icon src=(icon) alt=(name) loading="lazy" data-fallback-src="/logo/homarr.svg";
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
    // Like upstream, the custom title only shows when its toggle is on.
    let title = options
        .get("customTitleToggle")
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
        .then(|| options.get("customTitle").and_then(|v| v.as_str()))
        .flatten()
        .unwrap_or("");
    // Upstream's resolveClockTimeFormat: a custom dayjs format wins, else 24h or "hh:mm A".
    let time_format = options
        .get("customTimeFormat")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|f| !f.is_empty() && *f != "automatic")
        .unwrap_or(if is_24 { "HH:mm" } else { "hh:mm A" });
    let date_format = options
        .get("dateFormat")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .unwrap_or("dddd, MMMM D");
    let time_zone = options
        .get("useCustomTimezone")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
        .then(|| options.get("timezone").and_then(|v| v.as_str()))
        .flatten();

    html! {
        div.clock-widget data-24h=(is_24) data-time-format=(time_format)
            data-date-format=(date_format) data-tz=[time_zone] {
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

fn stat_pill(label: &str, value: impl maud::Render) -> Markup {
    html! { div.stat-pill { span.label { (label) } strong { (value) } } }
}

fn render_media_requests(kind: WidgetKind, options: &Value, data: &Value) -> Markup {
    if let Some(err_html) = render_error_box(data) {
        return err_html;
    }
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
    // An empty or missing filter means "all statuses", like upstream.
    let status_filter: Vec<&str> = options
        .get("statusFilter")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let recent_days = options
        .get("recentDays")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let cutoff = (recent_days > 0)
        .then(|| chrono::Utc::now() - chrono::Duration::days(recent_days.min(365) as i64));
    let new_tab = options
        .get("linksTargetNewTab")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let integration_id = str_field(data, "integrationId");
    let items: Vec<&Value> = data
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| {
            let status = str_field(item, "status");
            status_filter.is_empty() || status_filter.contains(&status)
        })
        .filter(|item| {
            let created = chrono::DateTime::parse_from_rfc3339(str_field(item, "createdAt"));
            cutoff.is_none_or(|cutoff| created.is_ok_and(|created| created >= cutoff))
        })
        .collect();
    if items.is_empty() {
        return empty_box("No media requests");
    }
    let link_target = if new_tab { "_blank" } else { "_self" };
    let link_rel = new_tab.then_some("noopener noreferrer");
    html! {
        ul.media-request-list {
            @for item in items.iter().take(20) {
                @let status = str_field(item, "status");
                @let title = Some(str_field(item, "title")).filter(|t| !t.is_empty()).unwrap_or("Unknown");
                @let href = safe_url(str_field(item, "href"));
                @let poster = safe_url(str_field(item, "posterUrl"));
                @let backdrop = safe_url(str_field(item, "backdropUrl"));
                @let year = str_field(item, "airDate").get(..4).unwrap_or("TBD");
                @let requested_by = Some(str_field(item, "requestedBy")).filter(|u| !u.is_empty()).unwrap_or("Unknown");
                @let input = serde_json::json!({ "requestId": str_field(item, "id") }).to_string();
                li.media-request-card {
                    @if backdrop != "#" {
                        img.media-card-backdrop src=(backdrop) alt="" loading="lazy" decoding="async" data-hide-on-error;
                    }
                    @if poster != "#" {
                        img.media-request-poster src=(poster) alt="" loading="lazy" decoding="async" data-hide-on-error;
                    }
                    div.media-request-body {
                        div.media-request-row {
                            div.media-request-meta {
                                span.media-request-year { (year) }
                                @if let Some((label, color)) = availability_badge(str_field(item, "availability")) {
                                    span class={ "media-badge c-" (color) } { (label) }
                                }
                            }
                            div.media-request-user {
                                span.media-avatar { (icon(ICON_USER)) }
                                span.media-truncate { (requested_by) }
                            }
                        }
                        div.media-request-row {
                            @if href != "#" {
                                a.media-request-title href=(href) target=(link_target) rel=[link_rel] title=(title) { (title) }
                            } @else {
                                span.media-request-title title=(title) { (title) }
                            }
                            @if status == "pending" && !integration_id.is_empty() {
                                div.media-request-actions {
                                    button.media-action.c-green type="button" title="Approve request" aria-label="Approve request" data-integration-action="mediaRequestApprove" data-integration-ids=(integration_id) data-action-input=(input) { (icon(ICON_THUMB_UP)) }
                                    button.media-action.c-red type="button" title="Decline request" aria-label="Decline request" data-integration-action="mediaRequestDecline" data-integration-ids=(integration_id) data-action-input=(input) { (icon(ICON_THUMB_DOWN)) }
                                }
                            } @else {
                                @let (label, color) = request_status_badge(status);
                                span class={ "media-badge c-" (color) } { (label) }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Upstream `mediaAvailabilityConfiguration` label and color.
fn availability_badge(availability: &str) -> Option<(&'static str, &'static str)> {
    Some(match availability {
        "unknown" => ("Unknown", "orange"),
        "pending" => ("Pending", "violet"),
        "processing" => ("Processing", "blue"),
        "requested" => ("Requested", "violet"),
        "partiallyAvailable" => ("Partial", "yellow"),
        "available" => ("Available", "green"),
        "blacklisted" => ("Blacklisted", "gray"),
        "deleted" => ("Deleted", "red"),
        _ => return None,
    })
}

/// Upstream `mediaRequestStatusConfiguration` label and color.
fn request_status_badge(status: &str) -> (&str, &'static str) {
    match status {
        "pending" => ("Pending", "blue"),
        "approved" => ("Approved", "green"),
        "declined" => ("Declined", "red"),
        "failed" => ("Failed", "red"),
        "completed" => ("Completed", "green"),
        other => (other, "gray"),
    }
}

/// Static Tabler icon markup.
fn icon(svg: &'static str) -> PreEscaped<&'static str> {
    // safe: only called with the trusted `ICON_*` SVG constants below.
    PreEscaped(svg)
}

const ICON_USER: &str = r#"<svg class="media-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M8 7a4 4 0 1 0 8 0a4 4 0 0 0 -8 0" /><path d="M6 21v-2a4 4 0 0 1 4 -4h4a4 4 0 0 1 4 4v2" /></svg>"#;
const ICON_THUMB_UP: &str = r#"<svg class="media-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M7 11v8a1 1 0 0 1 -1 1h-2a1 1 0 0 1 -1 -1v-7a1 1 0 0 1 1 -1h3a4 4 0 0 0 4 -4v-1a2 2 0 0 1 4 0v5h3a2 2 0 0 1 2 2l-1 5a2 3 0 0 1 -2 2h-7a3 3 0 0 1 -3 -3" /></svg>"#;
const ICON_THUMB_DOWN: &str = r#"<svg class="media-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M7 13v-8a1 1 0 0 0 -1 -1h-2a1 1 0 0 0 -1 1v7a1 1 0 0 0 1 1h3a4 4 0 0 1 4 4v1a2 2 0 0 0 4 0v-5h3a2 2 0 0 0 2 -2l-1 -5a2 3 0 0 0 -2 -2h-7a3 3 0 0 0 -3 3" /></svg>"#;
const ICON_CALENDAR: &str = r#"<svg class="media-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 7a2 2 0 0 1 2 -2h12a2 2 0 0 1 2 2v12a2 2 0 0 1 -2 2h-12a2 2 0 0 1 -2 -2v-12z" /><path d="M16 3v4" /><path d="M8 3v4" /><path d="M4 11h16" /></svg>"#;
const ICON_CLOCK: &str = r#"<svg class="media-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 12a9 9 0 1 0 18 0a9 9 0 0 0 -18 0" /><path d="M12 7v5l3 3" /></svg>"#;
const ICON_BOOK: &str = r#"<svg class="media-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 19a9 9 0 0 1 9 0a9 9 0 0 1 9 0" /><path d="M3 6a9 9 0 0 1 9 0a9 9 0 0 1 9 0" /><path d="M3 6l0 13" /><path d="M12 6l0 13" /><path d="M21 6l0 13" /></svg>"#;
const ICON_STAR: &str = r#"<svg class="media-icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M8.243 7.34l-6.38 .925l-.113 .023a1 1 0 0 0 -.44 1.684l4.622 4.499l-1.09 6.355l-.013 .11a1 1 0 0 0 1.464 .944l5.706 -3l5.693 3l.1 .046a1 1 0 0 0 1.352 -1.1l-1.091 -6.355l4.624 -4.5l.078 -.085a1 1 0 0 0 -.633 -1.62l-6.38 -.926l-2.852 -5.78a1 1 0 0 0 -1.794 0l-2.853 5.78z" /></svg>"#;
const ICON_VIDEO: &str = r#"<svg class="media-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M15 10l4.553 -2.276a1 1 0 0 1 1.447 .894v6.764a1 1 0 0 1 -1.447 .894l-4.553 -2.276v-4z" /><path d="M3 6m0 2a2 2 0 0 1 2 -2h8a2 2 0 0 1 2 2v8a2 2 0 0 1 -2 2h-8a2 2 0 0 1 -2 -2z" /></svg>"#;
const ICON_MOVIE: &str = r#"<svg class="media-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 4m0 2a2 2 0 0 1 2 -2h12a2 2 0 0 1 2 2v12a2 2 0 0 1 -2 2h-12a2 2 0 0 1 -2 -2z" /><path d="M8 4l0 16" /><path d="M16 4l0 16" /><path d="M4 8l4 0" /><path d="M4 16l4 0" /><path d="M4 12l16 0" /><path d="M16 8l4 0" /><path d="M16 16l4 0" /></svg>"#;
const ICON_TV: &str = r#"<svg class="media-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 7a2 2 0 0 1 2 -2h14a2 2 0 0 1 2 2v9a2 2 0 0 1 -2 2h-14a2 2 0 0 1 -2 -2v-9z" /><path d="M16 3l-4 4l-4 -4" /></svg>"#;
const ICON_HEADPHONES: &str = r#"<svg class="media-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 13m0 2a2 2 0 0 1 2 -2h1a2 2 0 0 1 2 2v3a2 2 0 0 1 -2 2h-1a2 2 0 0 1 -2 -2z" /><path d="M15 13m0 2a2 2 0 0 1 2 -2h1a2 2 0 0 1 2 2v3a2 2 0 0 1 -2 2h-1a2 2 0 0 1 -2 -2z" /><path d="M4 15v-3a8 8 0 0 1 16 0v3" /></svg>"#;
const ICON_PAUSE: &str = r#"<svg class="media-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 5m0 1a1 1 0 0 1 1 -1h2a1 1 0 0 1 1 1v12a1 1 0 0 1 -1 1h-2a1 1 0 0 1 -1 -1z" /><path d="M14 5m0 1a1 1 0 0 1 1 -1h2a1 1 0 0 1 1 1v12a1 1 0 0 1 -1 1h-2a1 1 0 0 1 -1 -1z" /></svg>"#;
const ICON_WIFI: &str = r#"<svg class="media-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 18l.01 0" /><path d="M9.172 15.172a4 4 0 0 1 5.656 0" /><path d="M6.343 12.343a8 8 0 0 1 11.314 0" /><path d="M3.515 9.515c4.686 -4.687 12.284 -4.687 17 0" /></svg>"#;
const ICON_WORLD: &str = r#"<svg class="media-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 12a9 9 0 1 0 18 0a9 9 0 0 0 -18 0" /><path d="M3.6 9h16.8" /><path d="M3.6 15h16.8" /><path d="M11.5 3a17 17 0 0 0 0 18" /><path d="M12.5 3a17 17 0 0 1 0 18" /></svg>"#;

/// Icon of an integration kind (the dashboard-icons URL upstream shows as the source avatar).
fn integration_icon_url(kind: &str) -> Option<&'static str> {
    homarr_definitions::IntegrationKind::from_str_opt(kind)
        .map(|kind| homarr_definitions::get_integration_def(kind).icon_url)
        .filter(|url| safe_url(url) != "#")
}

fn str_field<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
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

/// Disable durations offered by the DNS hole controls, in seconds (0 = until re-enabled).
const DNS_DISABLE_DURATIONS: [(u64, &str); 4] = [
    (300, "5 minutes"),
    (1800, "30 minutes"),
    (3600, "1 hour"),
    (0, "Until re-enabled"),
];

/// Disable button with a duration menu (`<details>`, no JS needed to open it).
fn dns_disable_menu(ids: &str, label: &str) -> Markup {
    html! {
        details.dns-disable-menu {
            summary.btn.btn-sm.btn-danger { (label) }
            div.dns-disable-options {
                @for (secs, text) in DNS_DISABLE_DURATIONS {
                    button.btn.btn-sm type="button" data-integration-action="dnsHoleDisable" data-integration-ids=(ids)
                        data-action-input=(serde_json::json!({ "duration": secs }).to_string()) { (text) }
                }
            }
        }
    }
}

fn render_dns_hole(kind: WidgetKind, options: &Value, data: &Value) -> Markup {
    let items: Vec<&Value> = data
        .get("items")
        .and_then(Value::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default();
    if kind == WidgetKind::DnsHoleControls {
        return render_dns_controls(options, &items);
    }

    // Summary: sum every reachable integration, like upstream.
    let summaries: Vec<&Value> = items.iter().filter_map(|i| i.get("summary")).collect();
    if summaries.is_empty() {
        let first_error = items.iter().find_map(|i| i.get("error")?.as_str());
        return empty_box(first_error.unwrap_or("DNS Hole metrics unavailable"));
    }
    let count = |key: &str| {
        summaries
            .iter()
            .map(|s| s.get(key).and_then(Value::as_u64).unwrap_or(0))
            .sum::<u64>()
    };
    let queries = count("dnsQueriesToday");
    let pct = if queries > 0 {
        count("adsBlockedToday") as f64 * 100.0 / queries as f64
    } else {
        0.0
    };
    let layout = match options.get("layout").and_then(Value::as_str) {
        Some("row") => "dns-layout-row",
        Some("column") => "dns-layout-column",
        _ => "",
    };

    html! {
        div class={ "dns-hole-grid " (layout) } {
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

fn is_dns_disabled(item: &Value) -> bool {
    item.pointer("/summary/status").and_then(Value::as_str) == Some("disabled")
}

fn render_dns_controls(options: &Value, items: &[&Value]) -> Markup {
    if items.is_empty() {
        return empty_box("DNS Hole metrics unavailable");
    }
    // Integrations that answered, split by state; failed ones are left out of "toggle all".
    let ids_where_disabled = |disabled: bool| {
        items
            .iter()
            .filter(|i| i.get("summary").is_some() && is_dns_disabled(i) == disabled)
            .map(|i| str_field(i, "integrationId"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let (disabled_ids, enabled_ids) = (ids_where_disabled(true), ids_where_disabled(false));
    let show_all = options
        .get("showToggleAllButtons")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    html! {
        @if show_all && (!disabled_ids.is_empty() || !enabled_ids.is_empty()) {
            div.widget-actions.dns-toggle-all {
                @if !disabled_ids.is_empty() {
                    button.btn.btn-sm.btn-primary type="button" data-integration-action="dnsHoleEnable" data-integration-ids=(disabled_ids) { "Enable all" }
                }
                @if !enabled_ids.is_empty() { (dns_disable_menu(&enabled_ids, "Disable all")) }
            }
        }
        ul.compact-list.dns-controls {
            @for item in items {
                @let id = str_field(item, "integrationId");
                @let name = str_field(item, "name");
                @if let Some(err) = item.get("error").and_then(Value::as_str) {
                    li.dns-control-row {
                        div { strong { (name) } div.metric-sub { (err) } }
                        span.badge.down { "error" }
                    }
                } @else {
                    @let enabled = !is_dns_disabled(item);
                    li.dns-control-row {
                        div {
                            strong { (name) } " "
                            span class={ "badge " (if enabled { "up" } else { "down" }) } { (if enabled { "enabled" } else { "disabled" }) }
                        }
                        @if enabled {
                            (dns_disable_menu(id, "Disable"))
                        } @else {
                            button.btn.btn-sm.btn-primary type="button" data-integration-action="dnsHoleEnable" data-integration-ids=(id) { "Enable" }
                        }
                    }
                }
            }
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
    if let Some(err_html) = render_error_box(data) {
        return err_html;
    }
    match data.get("health") {
        Some(h) => render_host_health(&Value::Null, h),
        None => empty_box("Health telemetry unavailable"),
    }
}

fn render_host_health(options: &Value, h: &Value) -> Markup {
    let fahrenheit = opt_flag(options, "fahrenheit", false);
    let show_cpu = opt_flag(options, "cpu", true);
    let show_memory = opt_flag(options, "memory", true);
    let show_disks = opt_flag(options, "fileSystem", true);
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
        .map(|t| format_temperature(t, fahrenheit))
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
        .filter(|d| show_disks && !d.is_empty());
    let nodes = h
        .get("nodes")
        .and_then(|v| v.as_array())
        .filter(|n| !n.is_empty());

    html! {
        div.health-host-row {
            strong { (name) }
            span.badge.up { "Up " (uptime) }
        }
        @if show_cpu || show_memory {
            div.health-gauges {
                @if show_cpu {
                    (render_donut_gauge("CPU", cpu, &cpu_sub, "var(--mantine-blue)"))
                }
                @if show_memory {
                    (render_donut_gauge("Memory", mem_pct, &mem_sub, "var(--mantine-violet)"))
                }
            }
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

fn str_at<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn f64_at(value: &Value, key: &str) -> Option<f64> {
    value.get(key).and_then(Value::as_f64)
}

/// Boolean widget option; missing or invalid values use the upstream default.
fn opt_flag(options: &Value, key: &str, default: bool) -> bool {
    options.get(key).and_then(Value::as_bool).unwrap_or(default)
}

/// One line per integration that failed in a multi-integration widget.
fn render_integration_errors(data: &Value) -> Markup {
    let errors = data.get("errors").and_then(Value::as_array);
    html! {
        @for err in errors.into_iter().flatten() {
            div.widget-error-line {
                span.dot.down {}
                strong { (str_at(err, "integrationName").unwrap_or("Integration")) }
                span.muted { (str_at(err, "error").unwrap_or("Unavailable")) }
            }
        }
    }
}

fn has_integration_errors(data: &Value) -> bool {
    data.get("errors")
        .and_then(Value::as_array)
        .is_some_and(|e| !e.is_empty())
}

fn format_temperature(celsius: f64, fahrenheit: bool) -> String {
    if fahrenheit {
        format!("{:.0}°F", celsius * 9.0 / 5.0 + 32.0)
    } else {
        format!("{celsius:.0}°C")
    }
}

fn render_health_monitoring(options: &Value, data: &Value) -> Markup {
    let items = data
        .get("items")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    // Without client-side tabs, the default tab decides which kind is shown first.
    let cluster_first = str_at(options, "defaultTab") == Some("cluster");
    let mut ordered: Vec<&Value> = items.iter().collect();
    ordered.sort_by_key(|item| item.get("cluster").is_some() != cluster_first);
    html! {
        (render_integration_errors(data))
        @if items.is_empty() && !has_integration_errors(data) {
            (empty_box("Health telemetry unavailable"))
        }
        @for item in ordered {
            div.health-item {
                @if let Some(cluster) = item.get("cluster") {
                    (render_cluster_health(options, cluster))
                } @else if let Some(health) = item.get("health") {
                    (render_host_health(options, health))
                }
            }
        }
    }
}

struct ClusterSection<'a> {
    title: &'static str,
    icon: &'static str,
    resources: Vec<&'a Value>,
    is_storage: bool,
}

// Tabler outline icon paths (static markup, never user data).
const ICON_CPU: &str = r#"<path d="M5 6a1 1 0 0 1 1 -1h12a1 1 0 0 1 1 1v12a1 1 0 0 1 -1 1h-12a1 1 0 0 1 -1 -1z"/><path d="M9 9h6v6h-6z"/><path d="M3 10h2"/><path d="M3 14h2"/><path d="M10 3v2"/><path d="M14 3v2"/><path d="M21 10h-2"/><path d="M21 14h-2"/><path d="M14 21v-2"/><path d="M10 21v-2"/>"#;
const ICON_BRAIN: &str = r#"<path d="M15.5 13a3.5 3.5 0 0 0 -3.5 3.5v1a3.5 3.5 0 0 0 7 0v-1.8"/><path d="M8.5 13a3.5 3.5 0 0 1 3.5 3.5v1a3.5 3.5 0 0 1 -7 0v-1.8"/><path d="M17.5 16a3.5 3.5 0 0 0 0 -7h-.5"/><path d="M19 9.3v-2.8a3.5 3.5 0 0 0 -7 0"/><path d="M6.5 16a3.5 3.5 0 0 1 0 -7h.5"/><path d="M5 9.3v-2.8a3.5 3.5 0 0 1 7 0v10"/>"#;
const ICON_SERVER: &str = r#"<path d="M3 7a3 3 0 0 1 3 -3h12a3 3 0 0 1 3 3v2a3 3 0 0 1 -3 3h-12a3 3 0 0 1 -3 -3z"/><path d="M3 15a3 3 0 0 1 3 -3h12a3 3 0 0 1 3 3v2a3 3 0 0 1 -3 3h-12a3 3 0 0 1 -3 -3z"/><path d="M7 8v.01"/><path d="M7 16v.01"/>"#;
const ICON_LAPTOP: &str = r#"<path d="M3 19h18"/><path d="M5 7a1 1 0 0 1 1 -1h12a1 1 0 0 1 1 1v8a1 1 0 0 1 -1 1h-12a1 1 0 0 1 -1 -1z"/>"#;
const ICON_CUBE: &str = r#"<path d="M21 16.008v-8.018a1.98 1.98 0 0 0 -1 -1.717l-7 -4.008a2.016 2.016 0 0 0 -2 0l-7 4.008c-.619 .355 -1 1.01 -1 1.718v8.018c0 .709 .381 1.363 1 1.717l7 4.008a2.016 2.016 0 0 0 2 0l7 -4.008c.619 -.355 1 -1.01 1 -1.718z"/><path d="M12 22v-10"/><path d="M12 12l8.73 -5.04"/><path d="M3.27 6.96l8.73 5.04"/>"#;
const ICON_DATABASE: &str = r#"<path d="M4 6a8 3 0 1 0 16 0a8 3 0 1 0 -16 0"/><path d="M4 6v6a8 3 0 0 0 16 0v-6"/><path d="M4 12v6a8 3 0 0 0 16 0v-6"/>"#;
const ICON_CHEVRON_DOWN: &str = r#"<path d="M6 9l6 6l6 -6"/>"#;
const ICON_CIRCLE_CHECK: &str =
    r#"<path d="M3 12a9 9 0 1 0 18 0a9 9 0 1 0 -18 0"/><path d="M9 12l2 2l4 -4"/>"#;
const ICON_ALERT_TRIANGLE: &str = r#"<path d="M12 9v4"/><path d="M10.363 3.591l-8.106 13.534a1.914 1.914 0 0 0 1.636 2.871h16.214a1.914 1.914 0 0 0 1.636 -2.87l-8.106 -13.536a1.914 1.914 0 0 0 -3.274 0z"/><path d="M12 16h.01"/>"#;
const ICON_CIRCLE_X: &str =
    r#"<path d="M3 12a9 9 0 1 0 18 0a9 9 0 1 0 -18 0"/><path d="M10 10l4 4m0 -4l-4 4"/>"#;
const ICON_TOOL: &str = r#"<path d="M7 10h3v-3l-3.5 -3.5a6 6 0 0 1 8 8l6 6a2 2 0 0 1 -3 3l-6 -6a6 6 0 0 1 -8 -8l3.5 3.5"/>"#;
const ICON_HELP_CIRCLE: &str = r#"<path d="M3 12a9 9 0 1 0 18 0a9 9 0 1 0 -18 0"/><path d="M12 16v.01"/><path d="M12 13a2 2 0 0 0 .914 -3.782a1.98 1.98 0 0 0 -2.414 .483"/>"#;
const ICON_EXTERNAL_LINK: &str = r#"<path d="M12 6h-6a2 2 0 0 0 -2 2v10a2 2 0 0 0 2 2h10a2 2 0 0 0 2 -2v-6"/><path d="M11 13l9 -9"/><path d="M15 4h5v5"/>"#;

fn tabler_icon(paths: &'static str) -> Markup {
    // safe: `paths` is always one of the static ICON_* consts above.
    PreEscaped(format!(
        r#"<svg class="ui-icon" viewBox="0 0 24 24" width="1em" height="1em" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">{paths}</svg>"#
    ))
}

/// Upstream's dayjs duration parts: whole years dropped, months of 365/12 days.
fn format_long_uptime(secs: u64) -> String {
    let rest = secs % 31_536_000;
    let months = rest / 2_628_000;
    let rest = rest % 2_628_000;
    let days = rest / 86_400;
    let hours = (rest % 86_400) / 3_600;
    let minutes = (rest % 3_600) / 60;
    format!("Uptime: {months} Months, {days} Days, {hours} Hours, {minutes} Minutes")
}

fn render_cluster_ring(icon: &'static str, label: &str, pct: f64) -> Markup {
    let clamped = pct.clamp(0.0, 100.0);
    let circ = 125.664_f64; // 2 * pi * r(20)
    let offset = circ * (1.0 - clamped / 100.0);
    let tone = if clamped > 75.0 { "high" } else { "ok" };
    html! {
        div.hm-ring {
            div.hm-ring-gauge {
                svg viewBox="0 0 48 48" width="48" height="48" aria-hidden="true" {
                    circle.hm-ring-track cx="24" cy="24" r="20" {}
                    circle.hm-ring-arc data-tone=(tone) cx="24" cy="24" r="20"
                        style=(format!("stroke-dasharray:{circ:.1};stroke-dashoffset:{offset:.1}")) {}
                }
                span.hm-ring-icon { (tabler_icon(icon)) }
            }
            div.hm-ring-text {
                span.hm-ring-label { (label) }
                span.hm-ring-value { (format!("{clamped:.1}%")) }
            }
        }
    }
}

fn render_cluster_health(options: &Value, cluster: &Value) -> Markup {
    let list = |key: &str| {
        cluster
            .get(key)
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
    };
    let nodes = list("nodes");
    let running_nodes: Vec<&Value> = nodes.iter().filter(|n| resource_running(n)).collect();
    let sum = |a: &str, b: &str| -> f64 { running_nodes.iter().map(|n| nested_f64(n, a, b)).sum() };
    let cores = sum("cpu", "cores");
    let used_cpu: f64 = running_nodes
        .iter()
        .map(|n| nested_f64(n, "cpu", "utilization") * nested_f64(n, "cpu", "cores"))
        .sum();
    let used_mem = sum("memory", "used");
    let total_mem = sum("memory", "total");
    let cpu_pct = if cores > 0.0 {
        used_cpu / cores * 100.0
    } else {
        0.0
    };
    let mem_pct = if total_mem > 0.0 {
        used_mem / total_mem * 100.0
    } else {
        0.0
    };
    let uptime = nodes
        .iter()
        .filter_map(|n| f64_at(n, "uptime"))
        .fold(0.0_f64, f64::max);

    let show_cpu = opt_flag(options, "cpu", true);
    let show_memory = opt_flag(options, "memory", true);
    let any_running_is_enough = str_at(options, "sectionIndicatorRequirement") == Some("any");
    let visible: Vec<&str> = options
        .get("visibleClusterSections")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_else(|| vec!["node", "qemu", "lxc", "storage"]);
    // Fixed upstream order; the option only picks which sections are shown.
    let sections: Vec<ClusterSection> = [
        ("node", "Nodes", ICON_SERVER, "nodes"),
        ("qemu", "VMs", ICON_LAPTOP, "vms"),
        ("lxc", "LXCs", ICON_CUBE, "lxcs"),
        ("storage", "Storage", ICON_DATABASE, "storages"),
    ]
    .into_iter()
    .filter(|(section, ..)| visible.contains(section))
    .map(|(_, title, icon, key)| {
        let mut resources: Vec<&Value> = list(key).iter().collect();
        resources.sort_by_key(|&r| {
            (
                str_at(r, "node").unwrap_or(""),
                str_at(r, "name").unwrap_or(""),
            )
        });
        ClusterSection {
            title,
            icon,
            resources,
            is_storage: key == "storages",
        }
    })
    .collect();

    html! {
        div.hm-cluster {
            @if opt_flag(options, "showUptime", true) {
                div.hm-uptime { (format_long_uptime(uptime as u64)) }
            }
            @if show_cpu || show_memory {
                div.hm-rings {
                    @if show_cpu {
                        (render_cluster_ring(ICON_CPU, "CPU", cpu_pct))
                    }
                    @if show_memory {
                        (render_cluster_ring(ICON_BRAIN, "RAM", mem_pct))
                    }
                }
            }
            @if !sections.is_empty() {
                div.hm-sections {
                    @for (index, section) in sections.iter().enumerate() {
                        @let active = section.resources.iter().filter(|r| resource_running(r)).count();
                        @let total = section.resources.len();
                        @let healthy = active == total || (any_running_is_enough && active > 0);
                        // Upstream opens only the first visible section by default.
                        details.hm-section open[index == 0] {
                            summary {
                                span.hm-section-icon { (tabler_icon(section.icon)) }
                                span.hm-section-title { (section.title) }
                                span.hm-count data-tone=(if healthy { "ok" } else { "warn" }) {
                                    span.hm-count-dot {}
                                    (active) " / " (total)
                                }
                                span.hm-chevron { (tabler_icon(ICON_CHEVRON_DOWN)) }
                            }
                            @if total > 0 {
                                table.hm-table {
                                    thead {
                                        tr {
                                            th { "Name" }
                                            @if section.is_storage {
                                                th { "Node" }
                                            } @else {
                                                th { "CPU" }
                                                th { "RAM" }
                                            }
                                        }
                                    }
                                    tbody {
                                        @for resource in &section.resources {
                                            tr {
                                                td {
                                                    span.hm-run-dot data-running=(if resource_running(resource) { "true" } else { "false" }) {}
                                                    (str_at(resource, "name").unwrap_or(""))
                                                }
                                                @if section.is_storage {
                                                    td { (str_at(resource, "node").unwrap_or("")) }
                                                } @else {
                                                    td { (format!("{:.1}%", nested_f64(resource, "cpu", "utilization") * 100.0)) }
                                                    td { (format_ratio_pct(nested_f64(resource, "memory", "used"), nested_f64(resource, "memory", "total"))) }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn resource_running(resource: &Value) -> bool {
    resource.get("isRunning").and_then(Value::as_bool) == Some(true)
}

fn nested_f64(value: &Value, outer: &str, inner: &str) -> f64 {
    value
        .get(outer)
        .and_then(|v| f64_at(v, inner))
        .unwrap_or(0.0)
}

fn monitor_status(monitor: &Value) -> &str {
    str_at(monitor, "status").unwrap_or("unknown")
}

/// Problems first, like upstream's status sort.
fn monitor_rank(monitor: &Value) -> u8 {
    match monitor_status(monitor) {
        "down" => 0,
        "pending" => 1,
        "maintenance" => 2,
        "up" => 4,
        _ => 3,
    }
}

fn monitor_name(monitor: &Value) -> String {
    str_at(monitor, "name").unwrap_or("").to_lowercase()
}

fn format_ratio_pct(used: f64, total: f64) -> String {
    if total > 0.0 {
        format!("{:.1}%", (used / total * 100.0).clamp(0.0, 100.0))
    } else {
        "-".to_string()
    }
}

/// Display order of the status breakdown, like upstream's `presentStatuses`.
const MONITOR_STATUSES: [&str; 6] = ["up", "down", "pending", "maintenance", "stale", "unknown"];

fn monitor_status_label(status: &str) -> &'static str {
    match status {
        "up" => "Up",
        "down" => "Down",
        "pending" => "Pending",
        "maintenance" => "Maintenance",
        "stale" => "No recent data",
        _ => "No data",
    }
}

fn is_inactive_status(status: &str) -> bool {
    matches!(status, "stale" | "unknown")
}

/// Upstream `formatUptime`: truncates, so 99.996% never shows as 100%.
fn format_kuma_uptime(uptime: f64) -> String {
    if uptime >= 100.0 {
        "100%".to_string()
    } else if uptime >= 99.0 {
        format!("{:.2}%", (uptime * 100.0).floor() / 100.0)
    } else {
        format!("{:.1}%", (uptime * 10.0).floor() / 10.0)
    }
}

fn uptime_tone(status: &str, uptime: Option<f64>) -> &'static str {
    match uptime {
        Some(u) if !is_inactive_status(status) => {
            if u >= 99.0 {
                "healthy"
            } else if u >= 95.0 {
                "warning"
            } else {
                "critical"
            }
        }
        _ => "inactive",
    }
}

fn render_uptime_kuma(options: &Value, data: &Value) -> Markup {
    let pages = data
        .get("pages")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    // A monitor can appear in several groups of one status page.
    let mut monitors: Vec<&Value> = Vec::new();
    let mut seen: Vec<(usize, i64)> = Vec::new();
    for (page_index, page) in pages.iter().enumerate() {
        let groups = page
            .pointer("/page/groups")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        for monitor in groups
            .iter()
            .filter_map(|g| g.get("monitors").and_then(Value::as_array))
            .flatten()
        {
            let key = (
                page_index,
                monitor.get("id").and_then(Value::as_i64).unwrap_or(-1),
            );
            if !seen.contains(&key) {
                seen.push(key);
                monitors.push(monitor);
            }
        }
    }
    let count = |status: &str| {
        monitors
            .iter()
            .filter(|m| monitor_status(m) == status)
            .count()
    };
    let total = monitors.len();
    let up = count("up");
    let down = count("down");
    let active: Vec<&Value> = monitors
        .iter()
        .copied()
        .filter(|m| !is_inactive_status(monitor_status(m)))
        .collect();
    let uptimes: Vec<f64> = active
        .iter()
        .filter_map(|m| f64_at(m, "uptime24h"))
        .collect();
    let average_uptime =
        (!uptimes.is_empty()).then(|| uptimes.iter().sum::<f64>() / uptimes.len() as f64);
    // Same precedence as the integration's `summarize`.
    let overall = if active.is_empty() {
        "unknown"
    } else if down == active.len() {
        "outage"
    } else if down + count("pending") > 0 {
        "degraded"
    } else if count("maintenance") > 0 {
        "maintenance"
    } else if active.len() < total {
        "unknown"
    } else {
        "operational"
    };
    let (overall_icon, overall_label) = match overall {
        "operational" => (ICON_CIRCLE_CHECK, "All systems operational".to_string()),
        "degraded" => (
            ICON_ALERT_TRIANGLE,
            "Partially degraded service".to_string(),
        ),
        "outage" => (ICON_CIRCLE_X, "Major outage".to_string()),
        "maintenance" => (ICON_TOOL, "Under maintenance".to_string()),
        _ if up > 0 => (ICON_HELP_CIRCLE, format!("{up} of {total} monitors up")),
        _ if count("stale") > 0 => (ICON_HELP_CIRCLE, "No recent data".to_string()),
        _ => (ICON_HELP_CIRCLE, "Waiting for first checks".to_string()),
    };
    let status_page = match pages {
        [page] => page.get("page"),
        _ => None,
    };
    let status_page_link = status_page
        .and_then(|p| str_at(p, "url"))
        .map(safe_url)
        .filter(|href| *href != "#");

    let show_summary = opt_flag(options, "showSummary", true);
    let show_uptime = opt_flag(options, "showUptime", true);
    let show_average = opt_flag(options, "showAverageUptime", show_uptime);
    let show_beats = opt_flag(options, "showHeartbeatBar", true);
    let show_ping = opt_flag(options, "showResponseTime", true);
    // Per-status count toggles carried over from older boards.
    let shows_count = |status: &str| match status {
        "up" => opt_flag(options, "showUpCount", true),
        "down" => opt_flag(options, "showDownCount", true),
        _ => opt_flag(options, "showPausedCount", true),
    };
    let present: Vec<(&str, usize)> = MONITOR_STATUSES
        .iter()
        .map(|&status| (status, count(status)))
        .filter(|(_, n)| *n > 0)
        .collect();

    let only_problems = opt_flag(options, "showOnlyProblems", false);
    let mut listed: Vec<&Value> = monitors
        .iter()
        .copied()
        .filter(|m| !only_problems || monitor_status(m) != "up")
        .collect();
    match str_at(options, "sortBy") {
        Some("name") => listed.sort_by_key(|m| monitor_name(m)),
        // Lowest uptime first; monitors without data last.
        Some("uptime") => listed.sort_by(|a, b| {
            f64_at(a, "uptime24h")
                .unwrap_or(101.0)
                .total_cmp(&f64_at(b, "uptime24h").unwrap_or(101.0))
        }),
        // Upstream's default: by status only (stable), keeping the status page order.
        _ => listed.sort_by_key(|m| monitor_rank(m)),
    }

    html! {
        (render_integration_errors(data))
        @if pages.is_empty() {
            @if !has_integration_errors(data) {
                (empty_box("No monitors"))
            }
        } @else {
            div.uk {
                @if show_summary {
                    header.uk-header {
                        div.uk-header-top {
                            span.uk-overall data-status=(overall) {
                                span.uk-overall-icon { (tabler_icon(overall_icon)) }
                                strong { (overall_label) }
                            }
                            @if let Some(href) = status_page_link {
                                a.uk-page-link href=(href) target="_blank" rel="noopener noreferrer" {
                                    span { (status_page.and_then(|p| str_at(p, "title")).unwrap_or("")) }
                                    (tabler_icon(ICON_EXTERNAL_LINK))
                                }
                            }
                        }
                        div.uk-distribution {
                            @for (status, n) in &present {
                                span.uk-distribution-part data-status=(status)
                                    style=(format!("flex-grow:{n}"))
                                    title=(format!("{n} {}", monitor_status_label(status))) {}
                            }
                        }
                        div.uk-summary-line {
                            span.uk-breakdown {
                                @for (status, n) in present.iter().filter(|entry| shows_count(entry.0)) {
                                    span.uk-breakdown-item {
                                        span.uk-dot data-status=(status) {}
                                        strong { (n) }
                                        span.muted { (monitor_status_label(status)) }
                                    }
                                }
                            }
                            @if let Some(uptime) = average_uptime.filter(|_| show_average) {
                                span.uk-metric {
                                    strong { (format_kuma_uptime(uptime)) }
                                    span.muted { "uptime 24h" }
                                }
                            }
                        }
                    }
                }
                @if !opt_flag(options, "summaryOnly", false) {
                    @if listed.is_empty() && total > 0 {
                        div.uk-all-clear {
                            (tabler_icon(ICON_CIRCLE_CHECK))
                            @if total == 1 {
                                span { "The monitor is up" }
                            } @else {
                                span { (format!("All {total} monitors are up")) }
                            }
                        }
                    }
                    div.uk-monitors {
                        @for monitor in listed {
                            @let state = monitor_status(monitor);
                            @let name = str_at(monitor, "name").unwrap_or("");
                            @let href = str_at(monitor, "url").map(safe_url).filter(|h| *h != "#");
                            @let uptime = f64_at(monitor, "uptime24h");
                            div.uk-monitor data-status=(state) {
                                div.uk-monitor-main {
                                    span.uk-dot data-status=(state) {}
                                    @if let Some(href) = href {
                                        a.uk-monitor-name href=(href) target="_blank" rel="noopener noreferrer" { (name) }
                                    } @else {
                                        span.uk-monitor-name { (name) }
                                    }
                                    @if state != "up" {
                                        span.uk-monitor-status { (monitor_status_label(state)) }
                                    }
                                }
                                @if show_beats {
                                    div.uk-beats aria-hidden="true" {
                                        @for beat in monitor.get("heartbeats").and_then(Value::as_array).into_iter().flatten() {
                                            @let beat_status = str_at(beat, "status").unwrap_or("unknown");
                                            @let ping = f64_at(beat, "ping").map(|p| format!(" · {p:.0} ms")).unwrap_or_default();
                                            span.uk-beat data-status=(beat_status)
                                                title=(format!("{}{ping}", monitor_status_label(beat_status))) {}
                                        }
                                    }
                                }
                                @if show_ping {
                                    span.uk-ping {
                                        @if let Some(ping) = f64_at(monitor, "ping") {
                                            (format!("{ping:.0} ms"))
                                        }
                                    }
                                }
                                @if show_uptime {
                                    span.uk-uptime data-tone=(uptime_tone(state, uptime)) {
                                        @if let Some(uptime) = uptime {
                                            (format_kuma_uptime(uptime))
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// `formatBitRate` from upstream: decimal units, one fraction digit, no trailing `.0`.
fn format_bitrate_kbps(kbps: f64) -> Option<String> {
    if !kbps.is_finite() || kbps <= 0.0 {
        return None;
    }
    let units = ["b/s", "Kb/s", "Mb/s", "Gb/s", "Tb/s"];
    let mut value = kbps * 1000.0;
    let mut index = 0;
    while value >= 999.95 && index < units.len() - 1 {
        value /= 1000.0;
        index += 1;
    }
    let text = format!("{value:.1}");
    let text = text.strip_suffix(".0").unwrap_or(&text);
    Some(format!("{text} {}", units[index]))
}

/// Resolution tier label as Plex/Jellyfin show it (a 1920x804 encode is still "1080p").
fn resolution_label(width: f64, height: f64) -> String {
    let (short, long) = if height > width {
        (width, height)
    } else {
        (height, width)
    };
    let effective = short.max((long * 9.0 / 16.0).round());
    [4320.0, 2160.0, 1440.0, 1080.0, 720.0, 480.0, 360.0, 240.0]
        .into_iter()
        .find(|tier| effective >= tier * 0.9)
        .map_or_else(|| format!("{effective:.0}p"), |tier| format!("{tier:.0}p"))
}

/// Upstream `getPlaybackStatus` label and badge color.
fn playback_status(transcoding: Option<&Value>) -> (&'static str, &'static str) {
    let Some(t) = transcoding else {
        return ("Direct play", "green");
    };
    let flag = |key: &str| t.get(key).and_then(Value::as_bool).unwrap_or(true);
    match (flag("isVideoDirect"), flag("isAudioDirect")) {
        (true, true) if t.get("containerChanged").and_then(Value::as_bool) == Some(true) => {
            ("Direct stream", "teal")
        }
        (true, true) => ("Direct play", "green"),
        (false, true) => ("Transcoding video", "orange"),
        (true, false) => ("Transcoding audio", "yellow"),
        (false, false) => ("Transcoding", "red"),
    }
}

fn render_media_sessions(options: &Value, data: &Value) -> Markup {
    let sessions = data
        .get("sessions")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let show_bitrate = opt_flag(options, "showBitrate", true);
    let show_location = opt_flag(options, "showLocation", true);
    let total_kbps: f64 = sessions
        .iter()
        .filter_map(|s| {
            s.pointer("/currentlyPlaying/metadata/bitrateKbps")?
                .as_f64()
        })
        .sum();
    let total_bitrate = format_bitrate_kbps(total_kbps).filter(|_| show_bitrate);
    // Integrations with at least one session, in order, like the upstream footer.
    let mut sources: Vec<(&str, &str)> = Vec::new();
    for s in sessions {
        let source = (
            str_at(s, "integrationName").unwrap_or(""),
            str_at(s, "integrationKind").unwrap_or(""),
        );
        if !source.0.is_empty() && !sources.contains(&source) {
            sources.push(source);
        }
    }
    let count = sessions.len();

    html! {
        div.media-server {
            (render_integration_errors(data))
            div.media-server-scroll {
                table.media-server-table {
                    thead {
                        tr {
                            th.media-col-user { "User" }
                            th { "Currently playing" }
                            th.media-col-status { "Status" }
                        }
                    }
                    tbody {
                        @for s in sessions {
                            (render_media_session(s, show_bitrate, show_location))
                        }
                    }
                }
            }
            div.media-server-footer {
                div.media-server-count {
                    (icon(ICON_VIDEO))
                    span { (count) " active stream" @if count != 1 { "s" } }
                    @if let Some(total) = &total_bitrate { span.muted { "Total: " (total) } }
                }
                div.media-server-sources {
                    @for (name, kind) in &sources {
                        span.media-server-source {
                            @if let Some(url) = integration_icon_url(kind) {
                                img src=(url) alt="" loading="lazy" data-hide-on-error;
                            }
                            span { (name) }
                        }
                    }
                }
            }
        }
    }
}

fn render_media_session(s: &Value, show_bitrate: bool, show_location: bool) -> Markup {
    let user = s
        .pointer("/user/username")
        .and_then(Value::as_str)
        .unwrap_or("Unknown user");
    let session_name = str_at(s, "sessionName").unwrap_or("");
    let user_cell = html! {
        td {
            div.media-user {
                span.media-avatar { (icon(ICON_USER)) }
                div.media-user-text {
                    span.media-truncate { (user) }
                    span.media-truncate.muted { (session_name) }
                }
            }
        }
    };
    let Some(p) = s.get("currentlyPlaying").filter(|p| !p.is_null()) else {
        return html! {
            tr {
                (user_cell)
                td { span.muted { (session_name) } }
                td {}
            }
        };
    };
    let title = str_at(p, "name").unwrap_or("");
    let season_episode = match (
        p.get("seasonNumber").and_then(Value::as_i64),
        p.get("episodeNumber").and_then(Value::as_i64),
    ) {
        (Some(season), Some(episode)) => Some(format!("S{season:02} E{episode:02}")),
        _ => None,
    };
    let paused = p.pointer("/playback/state").and_then(Value::as_str) == Some("paused");
    let position = p.pointer("/playback/positionMs").and_then(Value::as_f64);
    let duration = p
        .pointer("/playback/durationMs")
        .and_then(Value::as_f64)
        .filter(|d| *d > 0.0);
    let pct = position
        .zip(duration)
        .map(|(pos, dur)| (pos / dur * 100.0).clamp(0.0, 100.0));
    // The legacy mock reports a percentage, not milliseconds; its "duration" is exactly 100.
    let minutes_left = match (position, duration) {
        (Some(pos), Some(dur)) if dur != 100.0 => Some(((dur - pos) / 60_000.0).round().max(0.0)),
        _ => None,
    };
    let type_icon = if paused {
        ICON_PAUSE
    } else {
        match str_at(p, "type") {
            Some("movie") => ICON_MOVIE,
            Some("tv") => ICON_TV,
            Some("audio") => ICON_HEADPHONES,
            _ => ICON_VIDEO,
        }
    };
    let metadata = p.get("metadata").filter(|m| !m.is_null());
    let (status, status_color) = playback_status(
        metadata
            .and_then(|m| m.get("transcoding"))
            .filter(|t| !t.is_null()),
    );
    let resolution = metadata
        .and_then(|m| {
            m.pointer("/transcoding/resolution")
                .filter(|r| !r.is_null())
                .or_else(|| m.pointer("/video/resolution"))
        })
        .and_then(|r| Some(resolution_label(f64_at(r, "width")?, f64_at(r, "height")?)));
    let bitrate = metadata
        .and_then(|m| f64_at(m, "bitrateKbps"))
        .and_then(format_bitrate_kbps)
        .filter(|_| show_bitrate);
    let location = str_at(p, "location").filter(|_| show_location);

    html! {
        tr {
            (user_cell)
            td {
                div.media-now {
                    span.media-now-icon.is-paused[paused] { (icon(type_icon)) }
                    span.media-truncate { (title) }
                    @if let Some(se) = &season_episode { span.muted.media-nowrap { (se) } }
                    @if paused {
                        span.media-paused { "Paused" }
                    } @else if let Some(left) = minutes_left {
                        span.muted.media-nowrap { (format!("{left:.0} min left")) }
                    }
                }
                @if let Some(pct) = pct {
                    div.progress-track.media-progress {
                        div.progress-fill.(if paused { "is-yellow" } else { "is-green" }) style=(format!("width:{pct:.1}%")) {}
                    }
                }
            }
            td {
                div.media-status-line {
                    span class={ "media-badge c-" (status_color) } { (status) }
                    @if let Some(loc) = location {
                        span.muted.media-location {
                            (icon(if loc == "lan" { ICON_WIFI } else { ICON_WORLD }))
                            (loc.to_uppercase())
                        }
                    }
                }
                @if resolution.is_some() || bitrate.is_some() {
                    div.media-status-line.muted {
                        @if let Some(res) = &resolution { span { (res) } }
                        @if let Some(rate) = &bitrate { span { (rate) } }
                    }
                }
            }
        }
    }
}

fn render_media_releases(options: &Value, data: &Value) -> Markup {
    let releases = data
        .get("releases")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let poster_layout = str_at(options, "layout") == Some("poster");
    html! {
        (render_integration_errors(data))
        @if releases.is_empty() {
            @if !has_integration_errors(data) {
                (empty_box("No recent media releases"))
            }
        } @else {
            ul.media-releases.(if poster_layout { "is-poster" } else { "is-backdrop" }) {
                @for rel in releases {
                    li { (render_media_release(rel, options, poster_layout)) }
                }
            }
        }
    }
}

/// Upstream `mediaTypeConfigurations` color.
fn media_type_color(kind: &str) -> &'static str {
    match kind {
        "movie" => "blue",
        "tv" => "violet",
        "music" => "green",
        "book" => "orange",
        "game" => "yellow",
        "video" => "red",
        "article" => "pink",
        _ => "gray",
    }
}

fn render_media_release(rel: &Value, options: &Value, poster_layout: bool) -> Markup {
    let show_type = opt_flag(options, "showType", true);
    let show_source = opt_flag(options, "showSource", true);
    let description = str_at(rel, "description")
        .filter(|d| !d.trim().is_empty())
        .filter(|_| opt_flag(options, "showDescriptionTooltip", true));
    let title = str_at(rel, "title").unwrap_or("Untitled");
    let subtitle = str_at(rel, "subtitle").filter(|s| !s.trim().is_empty());
    let href = safe_url(str_at(rel, "href").unwrap_or("#"));
    let image = |key: &str| {
        rel.pointer(key)
            .and_then(Value::as_str)
            .map(safe_url)
            .filter(|u| *u != "#")
    };
    let backdrop = image("/imageUrls/backdrop").filter(|_| !poster_layout);
    let poster = image("/imageUrls/poster").filter(|_| poster_layout);
    let release_date = str_at(rel, "releaseDate").unwrap_or("");
    let date_only = rel
        .get("releaseDateIsDateOnly")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let kind = str_at(rel, "type").unwrap_or("unknown");
    let length = f64_at(rel, "length")
        .filter(|l| *l > 0.0)
        .and_then(|l| match kind {
            "movie" | "tv" | "video" | "music" | "article" => {
                Some((ICON_CLOCK, format!("{:.0}min", (l / 60.0).round())))
            }
            "book" => Some((ICON_BOOK, format!("{l:.0}"))),
            _ => None,
        });
    let tags: Vec<&str> = rel
        .get("tags")
        .and_then(Value::as_array)
        .map(|t| t.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let source_name = str_at(rel, "integrationName").unwrap_or("");
    let source_icon = integration_icon_url(str_at(rel, "integrationKind").unwrap_or(""));

    let content = html! {
        @if let Some(src) = backdrop {
            img.media-card-backdrop src=(src) alt="" loading="lazy" decoding="async" data-hide-on-error;
        }
        @if let Some(src) = poster {
            img.media-release-poster src=(src) alt=(title) loading="lazy" decoding="async" data-hide-on-error;
        }
        div.media-release-body {
            div.media-release-title { (title) }
            @if let Some(sub) = subtitle { div.media-release-sub { (sub) } }
            div.media-release-info {
                span {
                    (icon(ICON_CALENDAR))
                    time.js-local-date datetime=(release_date) data-date-only=[date_only.then_some("true")] {
                        (release_date.get(..10).unwrap_or(release_date))
                    }
                }
                @if let Some((length_icon, label)) = length {
                    span.media-dot { "•" }
                    span { (icon(length_icon)) (label) }
                }
                @if let Some(producer) = str_at(rel, "producer") {
                    span.media-dot { "•" }
                    span { (producer) }
                }
                @if let Some(rating) = str_at(rel, "rating") {
                    span.media-dot { "•" }
                    span { (icon(ICON_STAR)) (rating) }
                }
                @if let Some(price) = f64_at(rel, "price") {
                    span.media-dot { "•" }
                    span { (format!("${price:.2}")) }
                }
            }
            @if !tags.is_empty() {
                div.media-release-tags {
                    @for tag in tags.iter().take(3) { span.media-chip { (tag) } }
                    @if tags.len() > 3 { span.media-chip { "+" (tags.len() - 3) } }
                }
            }
        }
        @if show_type || show_source {
            div.media-release-side {
                @if show_type {
                    span class={ "media-badge c-" (media_type_color(kind)) } { (kind) }
                } @else {
                    span {}
                }
                @if show_source && !source_name.is_empty() {
                    @if let Some(url) = source_icon {
                        img.media-release-source src=(url) alt=(source_name) title=(source_name) loading="lazy" data-hide-on-error;
                    } @else {
                        span.media-chip title=(source_name) { (source_name) }
                    }
                }
            }
        }
    };
    html! {
        @if href == "#" {
            div.media-release title=[description] { (content) }
        } @else {
            a.media-release href=(href) target="_blank" rel="noopener noreferrer" title=[description] { (content) }
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
        // safe: the final HTML is sanitized with an ammonia allowlist (no scripts, handlers or
        // dangerous elements/URLs); this is the security boundary for Custom Widget templates.
        let sanitized = crate::sanitize::sanitize_custom_widget_html(rendered);
        return html! { div.custom-widget-content { (PreEscaped(sanitized)) } };
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

#[cfg(test)]
mod interactive_widget_tests {
    use super::*;
    use serde_json::json;

    fn summary(status: &str, blocked: u64, queries: u64, domains: u64) -> Value {
        json!({
            "status": status, "adsBlockedToday": blocked, "dnsQueriesToday": queries,
            "domainsBeingBlocked": domains, "adsPercentageToday": 99.0
        })
    }

    fn dns(kind: WidgetKind, options: Value, data: &Value) -> String {
        render_dns_hole(kind, &options, data).into_string()
    }

    fn requests(options: Value, data: &Value) -> String {
        let kind = WidgetKind::MediaRequestsRequestList;
        render_media_requests(kind, &options, data).into_string()
    }

    #[test]
    fn dns_controls_show_each_integration_state_and_errors() {
        let data = json!({ "items": [
            { "integrationId": "a", "name": "AdGuard 1", "summary": summary("enabled", 1, 2, 3) },
            { "integrationId": "b", "name": "AdGuard 2", "summary": summary("disabled", 1, 2, 3) },
            { "integrationId": "c", "name": "Pi-hole", "error": "connection refused" },
        ]});
        let html = dns(WidgetKind::DnsHoleControls, json!({}), &data);
        let enable_b = r#""dnsHoleEnable" data-integration-ids="b""#;
        let disable_a = r#""dnsHoleDisable" data-integration-ids="a""#;
        assert!(html.contains(enable_b), "{html}");
        assert!(html.contains(disable_a), "{html}");
        assert!(html.contains("connection refused"));
        // The failed integration gets no toggle.
        assert!(!html.contains(r#"data-integration-ids="c""#));
        assert!(html.contains("Enable all"));
        assert!(html.contains("Disable all"));

        let options = json!({ "showToggleAllButtons": false });
        let hidden = dns(WidgetKind::DnsHoleControls, options, &data);
        assert!(!hidden.contains("Enable all"));
    }

    #[test]
    fn dns_summary_sums_integrations() {
        let data = json!({ "items": [
            { "integrationId": "a", "name": "A", "summary": summary("enabled", 100, 1000, 192_000) },
            { "integrationId": "b", "name": "B", "summary": summary("enabled", 300, 1000, 192_000) },
            { "integrationId": "c", "name": "C", "error": "down" },
        ]});
        let html = dns(WidgetKind::DnsHoleSummary, json!({}), &data);
        assert!(html.contains("384.0k"), "{html}");
        assert!(html.contains("20.0%"), "{html}");
        assert!(html.contains("2.0k"), "{html}");
    }

    #[test]
    fn media_request_list_filters_and_offers_approval() {
        let data = json!({ "integrationId": "s1", "items": [
            { "id": "7", "title": "Andor", "status": "pending", "requestedBy": "alex",
              "createdAt": "2024-01-01T00:00:00Z", "href": "https://req.example/tv/1",
              "airDate": "2022-09-21T00:00:00Z", "availability": "unknown" },
            { "id": "8", "title": "Dune", "status": "declined", "requestedBy": "maria",
              "createdAt": "2024-01-01T00:00:00Z" },
        ]});
        let html = requests(json!({}), &data);
        assert!(html.contains(">2022<"));
        assert!(html.contains("Dune") && html.contains(">Declined<"));
        assert!(html.contains("alex") && html.contains(">Unknown<"));
        assert!(html.contains("mediaRequestApprove") && html.contains("mediaRequestDecline"));
        assert!(html.contains(r#"aria-label="Approve request""#));
        assert!(html.contains("{&quot;requestId&quot;:&quot;7&quot;}"));

        let filtered = requests(json!({ "statusFilter": ["declined"] }), &data);
        assert!(!filtered.contains("Andor"));
        assert!(filtered.contains("Dune"));

        let recent = requests(json!({ "recentDays": 7 }), &data);
        assert!(!recent.contains("Andor"));
    }
}

#[cfg(test)]
mod data_widget_tests {
    use homarr_definitions::WidgetKind;
    use homarr_integrations::capabilities::health_monitoring::ClusterHealthMonitoring;
    use serde_json::{json, Value};

    use super::render_widget_html;

    fn compute(kind: &str, name: &str, running: bool, cpu: f64, mem: f64) -> Value {
        json!({
            "type": kind,
            "name": name,
            "node": "pve",
            "id": format!("{kind}/{name}"),
            "isRunning": running,
            "status": if running { "running" } else { "stopped" },
            "cpu": { "utilization": cpu, "cores": 4.0 },
            "memory": { "used": mem, "total": 8.0 },
            "storage": { "used": 1.0, "total": 2.0 },
            "network": {},
            "uptime": 90_061.0,
        })
    }

    #[test]
    fn proxmox_cluster_health_renders_nodes_and_sections() {
        let mut vm_running = compute("qemu", "web", true, 0.1, 2.0);
        vm_running["vmId"] = json!(100);
        let mut vm_stopped = compute("qemu", "old", false, 0.0, 0.0);
        vm_stopped["vmId"] = json!(101);
        let mut lxc = compute("lxc", "dns", true, 0.05, 1.0);
        lxc["vmId"] = json!(102);
        // Round-trip through the integration type so the fixture matches its serialization.
        let cluster: ClusterHealthMonitoring = serde_json::from_value(json!({
            "nodes": [compute("node", "pve", true, 0.25, 4.0)],
            "vms": [vm_running, vm_stopped],
            "lxcs": [lxc],
            "storages": [],
        }))
        .unwrap();
        let options = json!({
            "cpu": true,
            "memory": true,
            "showUptime": true,
            "visibleClusterSections": ["node", "qemu", "lxc"],
            "defaultTab": "system",
            "sectionIndicatorRequirement": "all",
        });
        let data = json!({
            "items": [{ "integrationName": "Proxmox", "integrationKind": "proxmox", "cluster": cluster }],
            "errors": [{ "integrationName": "Glances", "error": "Unable to connect" }],
        });
        let html = render_widget_html(WidgetKind::HealthMonitoring, "i", &options, &data);
        assert!(
            html.contains("Uptime: 0 Months, 1 Days, 1 Hours, 1 Minutes"),
            "{html}"
        );
        assert!(html.contains("CPU") && html.contains("RAM"));
        assert!(
            html.contains("25.0%") && html.contains("50.0%"),
            "rings and node row: {html}"
        );
        assert!(html.contains("Nodes") && html.contains("VMs") && html.contains("LXCs"));
        assert!(html.contains("1 / 2"), "VMs x / y: {html}");
        // Only the first section starts expanded.
        assert_eq!(html.matches("<details").count(), 3);
        assert_eq!(html.matches(" open>").count(), 1, "{html}");
        assert!(!html.contains("Storage"));
        // A failing integration shows an error line instead of disappearing.
        assert!(html.contains("Glances") && html.contains("Unable to connect"));
        assert!(!html.contains("telemetry unavailable"));
    }

    #[test]
    fn uptime_kuma_renders_summary_and_monitors() {
        let monitor = |id: i64, name: &str, status: &str, uptime: f64| {
            json!({
                "id": id,
                "name": name,
                "status": status,
                "uptime24h": uptime,
                "ping": 12.0,
                "tags": [],
                "heartbeats": [
                    { "status": "up", "time": 1, "ping": 10.0 },
                    { "status": "down", "time": 2, "ping": null },
                ],
            })
        };
        let page = |monitors: Vec<Value>| {
            json!({
                "integrationName": "Kuma",
                "page": {
                    "title": "Home",
                    "url": "https://kuma.example/status/home",
                    "groups": [
                        { "id": 1, "name": "A", "monitors": monitors.clone() },
                        { "id": 2, "name": "B", "monitors": monitors },
                    ],
                    "incidents": [],
                    "maintenances": [],
                },
            })
        };
        let all_up = json!({
            "pages": [page(vec![
                monitor(1, "Router", "up", 100.0),
                monitor(2, "NAS", "up", 99.0),
            ])],
            "errors": [],
        });
        let html = render_widget_html(WidgetKind::UptimeKuma, "i", &json!({}), &all_up);
        assert!(html.contains("All systems operational"), "{html}");
        assert!(html.contains(r#"href="https://kuma.example/status/home""#));
        assert!(html.contains("<strong>2</strong>") && html.contains("uptime 24h"));
        assert!(html.contains("99.50%") && html.contains("Router") && html.contains("12 ms"));
        // Monitors are deduplicated across groups; each has its heartbeat strip.
        assert_eq!(html.matches(r#"class="uk-monitor""#).count(), 2);
        assert_eq!(html.matches(r#"class="uk-beat""#).count(), 4);

        let options = json!({
            "showUpCount": false,
            "showAverageUptime": false,
            "showHeartbeatBar": false,
        });
        let html = render_widget_html(WidgetKind::UptimeKuma, "i", &options, &all_up);
        assert!(!html.contains(r#"<span class="muted">Up</span>"#), "{html}");
        assert!(!html.contains("uptime 24h") && !html.contains("uk-beat"));

        let options = json!({ "showOnlyProblems": true });
        let html = render_widget_html(WidgetKind::UptimeKuma, "i", &options, &all_up);
        assert!(html.contains("All 2 monitors are up"), "{html}");

        let one_down = json!({
            "pages": [page(vec![
                monitor(1, "Router", "up", 100.0),
                monitor(2, "NAS", "down", 90.0),
            ])],
        });
        let options = json!({ "showOnlyProblems": true });
        let html = render_widget_html(WidgetKind::UptimeKuma, "i", &options, &one_down);
        assert!(html.contains("Partially degraded service"), "{html}");
        assert!(html.contains("NAS") && !html.contains("Router"));
        assert!(
            html.contains(r#"data-tone="critical""#),
            "90% uptime: {html}"
        );
    }

    #[test]
    fn media_server_shows_session_details_and_errors() {
        let data = json!({
            "sessions": [{
                "sessionId": "1",
                "sessionName": "Living room",
                "integrationName": "Plex",
                "user": { "userId": "u", "username": "alice" },
                "currentlyPlaying": {
                    "type": "tv",
                    "name": "Show",
                    "seasonNumber": 1,
                    "episodeNumber": 2,
                    "episodeName": "Pilot",
                    "location": "wan",
                    "playback": { "state": "playing", "positionMs": 600_000.0, "durationMs": 1_800_000.0 },
                    "metadata": {
                        "video": { "resolution": { "width": 1920, "height": 804 } },
                        "audio": {},
                        "transcoding": {
                            "target": {},
                            "isVideoDirect": false,
                            "isAudioDirect": true,
                            "containerChanged": true,
                        },
                        "bitrateKbps": 5400.0,
                    },
                },
            }],
            "errors": [{ "integrationName": "Jellyfin", "error": "timed out" }],
        });
        let html = render_widget_html(WidgetKind::MediaServer, "i", &json!({}), &data);
        for needle in [
            "Currently playing",
            "alice",
            "Living room",
            "S01 E02",
            "20 min left",
            "Transcoding video",
            "WAN",
            "1080p",
            "1 active stream<",
            "Total: 5.4 Mb/s",
            "Jellyfin",
            "timed out",
        ] {
            assert!(html.contains(needle), "missing {needle}: {html}");
        }

        // Nothing playing still shows the table and the upstream footer.
        let empty = render_widget_html(WidgetKind::MediaServer, "i", &json!({}), &json!({}));
        assert!(empty.contains("<th>Currently playing</th>"), "{empty}");
        assert!(empty.contains("0 active streams"), "{empty}");
    }

    #[test]
    fn media_release_card_shows_upstream_metadata() {
        let data = json!({
            "releases": [{
                "id": "1",
                "type": "movie",
                "title": "The Odyssey",
                "subtitle": "Defy the gods.",
                "releaseDate": "2026-07-15T00:00:00Z",
                "releaseDateIsDateOnly": true,
                "imageUrls": { "poster": null, "backdrop": "/api/image-proxy/abc" },
                "producer": "Universal Pictures",
                "rating": "9.4",
                "length": 10380.0,
                "tags": ["Action", "Adventure", "Drama", "Fantasy"],
                "href": "https://plex.example/web",
                "integrationName": "Plex",
                "integrationKind": "plex",
            }],
        });
        let html = render_widget_html(WidgetKind::MediaReleases, "i", &json!({}), &data);
        for needle in [
            r#"src="/api/image-proxy/abc""#,
            "Defy the gods.",
            r#"datetime="2026-07-15T00:00:00Z" data-date-only="true""#,
            "173min",
            "Universal Pictures",
            "9.4",
            ">Action<",
            ">+1<",
            "c-blue",
            r#"alt="Plex""#,
        ] {
            assert!(html.contains(needle), "missing {needle}: {html}");
        }
    }

    #[test]
    fn media_releases_honour_source_and_type_options() {
        let data = json!({
            "releases": [{
                "id": "1",
                "type": "movie",
                "title": "Film",
                "releaseDate": "2024-03-01T00:00:00Z",
                "imageUrls": { "poster": null, "backdrop": null },
                "tags": ["Drama"],
                "href": "",
                "rating": "8.1",
                "integrationName": "Jellyfin",
            }],
        });
        let html = render_widget_html(WidgetKind::MediaReleases, "i", &json!({}), &data);
        assert!(html.contains("Jellyfin") && html.contains(">movie<") && html.contains("8.1"));
        let options = json!({ "showSource": false, "showType": false });
        let html = render_widget_html(WidgetKind::MediaReleases, "i", &options, &data);
        assert!(!html.contains("Jellyfin") && !html.contains(">movie<"));
    }
}
