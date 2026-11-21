use homarr_api::AppState;
use homarr_auth::AuthSession;
use homarr_db::models::BoardRecord;
use homarr_db::repos::{apps, search_engines, settings};
use homarr_db::Database;
use homarr_definitions::settings::parse_branding_settings;
use homarr_widgets::safe_url;
use maud::{html, Markup, PreEscaped, DOCTYPE};
use serde_json::{json, Value};

use crate::i18n::I18nCatalog;
use crate::icons::{self, icon};

pub struct ServerUiSettings {
    pub app_name: String,
    pub default_color_scheme: String,
    pub logo_image_url: Option<String>,
    pub favicon_image_url: Option<String>,
    pub robots_meta: String,
}

impl ServerUiSettings {
    pub async fn load(db: &Database) -> Self {
        let raw = settings::get_all_server_settings(db)
            .await
            .unwrap_or_default();
        let branding = raw
            .get("branding")
            .map(parse_branding_settings)
            .unwrap_or_default();
        let scheme = raw
            .get("appearance")
            .and_then(|v| v.get("defaultColorScheme"))
            .and_then(Value::as_str)
            .unwrap_or("dark")
            .to_ascii_lowercase();
        let crawl = raw.get("crawlingAndIndexing");
        let mut robots = Vec::new();
        if crawl
            .and_then(|v| v.get("noIndex"))
            .and_then(Value::as_bool)
            .unwrap_or(true)
        {
            robots.push("noindex");
        }
        if crawl
            .and_then(|v| v.get("noFollow"))
            .and_then(Value::as_bool)
            .unwrap_or(true)
        {
            robots.push("nofollow");
        }
        if crawl
            .and_then(|v| v.get("noTranslate"))
            .and_then(Value::as_bool)
            .unwrap_or(true)
        {
            robots.push("notranslate");
        }
        Self {
            app_name: branding.app_name,
            default_color_scheme: if scheme == "light" {
                "light".to_string()
            } else {
                "dark".to_string()
            },
            logo_image_url: branding.logo_image_url.filter(|s| !s.trim().is_empty()),
            favicon_image_url: branding.favicon_image_url.filter(|s| !s.trim().is_empty()),
            robots_meta: robots.join(", "),
        }
    }

    pub fn apply<'a>(&'a self, ctx: &mut PageContext<'a>) {
        ctx.app_name = Some(&self.app_name);
        ctx.default_color_scheme = Some(&self.default_color_scheme);
        if ctx.logo_image_url.is_none() {
            ctx.logo_image_url = self.logo_image_url.as_deref();
        }
        if ctx.favicon_image_url.is_none() {
            ctx.favicon_image_url = self.favicon_image_url.as_deref();
        }
        if !self.robots_meta.is_empty() {
            ctx.robots_meta = Some(&self.robots_meta);
        }
    }
}

pub(crate) async fn build_spotlight_items(
    state: &AppState,
    visible_boards: &[BoardRecord],
    is_admin: bool,
    can_edit_board: bool,
) -> Vec<Value> {
    let mut items = Vec::new();
    for b in visible_boards {
        items.push(json!({
            "label": b.name,
            "sub": "Board",
            "category": "Boards",
            "href": format!("/boards/{}", urlencoding(&b.name)),
        }));
    }
    // Non-admins only see apps placed on public boards among `visible_boards`: some callers
    // pass every board here, so only boards everyone may view are trusted.
    // shortcut: hides apps on private boards a non-admin can view; pass the session in to fix.
    let shown_apps = if is_admin {
        apps::list_all(&state.db).await.unwrap_or_default()
    } else {
        let public: Vec<BoardRecord> = visible_boards
            .iter()
            .filter(|b| b.is_public)
            .cloned()
            .collect();
        let ids = homarr_api::routes::apps::app_ids_on_boards(state, &public)
            .await
            .unwrap_or_default();
        apps::list_all(&state.db)
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|a| ids.contains(&a.id))
            .collect()
    };
    for app in shown_apps {
        let href = app.href.as_deref().map(safe_url).unwrap_or("#").to_string();
        items.push(json!({
            "label": app.name,
            "sub": app.description.unwrap_or_else(|| "Application".to_string()),
            "category": "Apps",
            "href": href,
        }));
    }
    for se in search_engines::list_all(&state.db)
        .await
        .unwrap_or_default()
    {
        let href = se
            .url_template
            .as_deref()
            .map(safe_url)
            .filter(|u| *u != "#")
            .unwrap_or("https://duckduckgo.com")
            .to_string();
        items.push(json!({
            "label": format!("!{} ({})", se.short, se.name),
            "sub": "Search Engine",
            "category": "Web",
            "href": href,
        }));
    }
    items.push(json!({
        "label": "Toggle Color Scheme",
        "sub": "Switch dark / light theme",
        "category": "Actions",
        "action": "toggle-theme",
    }));
    if can_edit_board {
        items.push(json!({
            "label": "Toggle Edit Mode",
            "sub": "Edit board layout and widgets",
            "category": "Actions",
            "action": "edit-board",
        }));
    }
    items.push(json!({ "label": "Home Board", "sub": "Board", "category": "Boards", "href": "/" }));
    if is_admin {
        for (label, href) in [
            ("Manage Overview", "/manage"),
            ("Manage Boards", "/manage/boards"),
            ("Manage Apps", "/manage/apps"),
            ("Manage Integrations", "/manage/integrations"),
            ("Manage Search Engines", "/manage/search-engines"),
            ("Manage Custom Widgets", "/manage/custom-widgets"),
            ("Server Settings", "/manage/settings"),
            ("Manage Users", "/manage/users"),
            ("Manage Groups", "/manage/users/groups"),
            ("Manage Invites", "/manage/users/invites"),
            ("Docker Containers", "/manage/tools/docker"),
            ("Cron Tasks", "/manage/tools/tasks"),
            ("Server Logs", "/manage/tools/logs"),
            ("Certificates", "/manage/tools/certificates"),
            ("API Keys", "/manage/tools/api-keys"),
            ("Backup & Restore", "/manage/tools/backup"),
            ("AI Assistant", "/manage/assistant"),
            ("About Homarr", "/manage/about"),
        ] {
            items.push(
                json!({ "label": label, "sub": "Manage", "category": "Pages", "href": href }),
            );
        }
    }
    items.push(json!({ "label": "API Reference", "sub": "OpenAPI & MCP", "category": "Pages", "href": "/reference/api" }));
    items
}

pub struct PageContext<'a> {
    pub title: &'a str,
    pub body: Markup,
    pub session: Option<&'a AuthSession>,
    pub demo_mode: bool,
    pub demo_read_only: bool,
    pub boards: &'a [BoardRecord],
    pub active_board_name: Option<&'a str>,
    pub locale: &'a str,
    pub i18n: &'a I18nCatalog,
    pub custom_css: Option<&'a str>,
    pub primary_color: Option<&'a str>,
    pub secondary_color: Option<&'a str>,
    pub background_image_url: Option<&'a str>,
    pub background_image_attachment: Option<&'a str>,
    pub background_image_repeat: Option<&'a str>,
    pub background_image_size: Option<&'a str>,
    pub opacity: Option<i32>,
    pub item_radius: Option<&'a str>,
    pub logo_image_url: Option<&'a str>,
    pub favicon_image_url: Option<&'a str>,
    pub spotlight_items: &'a [Value],
    pub can_edit_board: bool,
    pub is_board_page: bool,
    pub hide_header: bool,
    pub app_name: Option<&'a str>,
    pub default_color_scheme: Option<&'a str>,
    pub robots_meta: Option<&'a str>,
}

impl<'a> PageContext<'a> {
    pub fn minimal(title: &'a str, body: Markup, locale: &'a str, i18n: &'a I18nCatalog) -> Self {
        Self {
            title,
            body,
            session: None,
            demo_mode: false,
            demo_read_only: false,
            boards: &[],
            active_board_name: None,
            locale,
            i18n,
            custom_css: None,
            primary_color: None,
            secondary_color: None,
            background_image_url: None,
            background_image_attachment: None,
            background_image_repeat: None,
            background_image_size: None,
            opacity: None,
            item_radius: None,
            logo_image_url: None,
            favicon_image_url: None,
            spotlight_items: &[],
            can_edit_board: false,
            is_board_page: false,
            hide_header: false,
            app_name: None,
            default_color_scheme: None,
            robots_meta: None,
        }
    }
}

fn sanitize_css(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("</style")
        || lower.contains("<script")
        || lower.contains("javascript:")
        || lower.contains("expression(")
    {
        return String::new();
    }
    raw.to_string()
}

fn sanitize_hex_color(raw: Option<&str>) -> Option<String> {
    let s = raw?.trim();
    if s.starts_with('#')
        && (s.len() == 4 || s.len() == 7)
        && s[1..].chars().all(|c| c.is_ascii_hexdigit())
    {
        Some(s.to_string())
    } else {
        None
    }
}

fn sanitize_css_url(raw: Option<&str>) -> Option<String> {
    let candidate = raw?.trim();
    if candidate.is_empty() {
        return None;
    }
    let safe = safe_url(candidate);
    if safe == "#"
        || safe.contains('\'')
        || safe.contains('"')
        || safe.contains('(')
        || safe.contains(')')
        || safe.contains('\\')
    {
        return None;
    }
    Some(safe.to_string())
}

fn map_item_radius(raw: Option<&str>) -> Option<&'static str> {
    match raw?.trim().to_ascii_lowercase().as_str() {
        "xs" => Some("4px"),
        "sm" => Some("6px"),
        "md" => Some("8px"),
        "lg" => Some("12px"),
        "xl" => Some("16px"),
        _ => None,
    }
}

fn sanitize_bg_attachment(raw: Option<&str>) -> &'static str {
    match raw.unwrap_or("fixed").trim().to_ascii_lowercase().as_str() {
        "scroll" => "scroll",
        "local" => "local",
        _ => "fixed",
    }
}

fn sanitize_bg_repeat(raw: Option<&str>) -> &'static str {
    match raw
        .unwrap_or("no-repeat")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "repeat" => "repeat",
        "repeat-x" => "repeat-x",
        "repeat-y" => "repeat-y",
        _ => "no-repeat",
    }
}

fn sanitize_bg_size(raw: Option<&str>) -> &'static str {
    match raw.unwrap_or("cover").trim().to_ascii_lowercase().as_str() {
        "contain" => "contain",
        "auto" => "auto",
        _ => "cover",
    }
}

pub(crate) fn urlencoding(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for b in raw.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// JSON safe to embed in an inline `<script>`.
pub fn script_json(value: &Value) -> String {
    serde_json::to_string(value)
        .unwrap_or_else(|_| "null".to_string())
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

/// Wraps a pre-rendered HTML string as a page body.
// shortcut: compat for pages not yet ported to Maud, remove once all pages use Markup
pub fn legacy_body(html: &str) -> Markup {
    // Safe raw HTML: callers build it with html_escape on every interpolated value.
    PreEscaped(html.to_string())
}

/// Embeds `value` as `<script type="application/json" id=…>`; read it in JS with
/// `JSON.parse(document.getElementById(id).textContent)`.
pub fn json_script(id: &str, value: &Value) -> Markup {
    html! {
        // Safe raw: script_json escapes `<`, `>` and `&`, so the data cannot close the tag.
        script type="application/json" id=(id) { (PreEscaped(script_json(value))) }
    }
}

fn locale_options(ctx: &PageContext<'_>) -> Markup {
    html! {
        @for code in ctx.i18n.codes() {
            option value=(code) selected[*code == ctx.locale] { (code) }
        }
    }
}

fn board_switcher(ctx: &PageContext<'_>) -> Markup {
    let active_label = ctx
        .active_board_name
        .or_else(|| ctx.boards.first().map(|b| b.name.as_str()))
        .unwrap_or("Boards");
    html! {
        div.dropdown {
            button.board-switcher-btn type="button" onclick="window.toggleDropdown(this)" aria-expanded="false" aria-label="Switch Board" {
                (icon(icons::LAYOUT_DASHBOARD))
                span.board-switcher-label { (active_label) }
                (icon(icons::CHEVRON_DOWN))
            }
            div.dropdown-menu.dropdown-menu-left {
                div.dropdown-header { "Boards" }
                @for b in ctx.boards {
                    @let is_active = ctx.active_board_name == Some(b.name.as_str());
                    a.dropdown-item.is-active[is_active] href={ "/boards/" (urlencoding(&b.name)) } {
                        span { (b.name) }
                        span.dropdown-check { @if is_active { (icon(icons::CHECK)) } }
                    }
                }
                @if ctx.session.is_some_and(AuthSession::is_admin) {
                    div.dropdown-divider {}
                    a.dropdown-item href="/manage/boards" { (icon(icons::PLUS)) span { "Manage Boards" } }
                }
            }
        }
    }
}

fn user_menu(ctx: &PageContext<'_>, sess: &AuthSession) -> Markup {
    let name = sess.username.as_deref().unwrap_or("User");
    let initial = name.chars().next().unwrap_or('U').to_ascii_uppercase();
    html! {
        div.dropdown {
            button.user-trigger type="button" onclick="window.toggleDropdown(this)" aria-expanded="false" aria-label="User menu" {
                span.avatar-circle { (initial) }
                span.user-trigger-name { (name) }
                (icon(icons::CHEVRON_DOWN))
            }
            div.dropdown-menu.dropdown-menu-right {
                div.dropdown-header { "Signed in as " strong { (name) } }
                @if sess.is_admin() {
                    a.dropdown-item href="/manage" { (icon(icons::SHIELD)) span { "Manage Workspace" } }
                }
                @if ctx.can_edit_board {
                    @if let Some(active) = ctx.active_board_name {
                        a.dropdown-item href={ "/boards/" (urlencoding(active)) "/settings" } {
                            (icon(icons::SETTINGS)) span { "Board Settings" }
                        }
                    }
                }
                a.dropdown-item href="/reference/api" { (icon(icons::BOOK)) span { "API Reference" } }
                @if !ctx.i18n.codes().is_empty() {
                    div.dropdown-item.dropdown-item-static {
                        (icon(icons::GLOBE))
                        span style="flex:1" { "Language" }
                        select.locale-select aria-label="Language" onchange="window.setLocaleCookie(this.value)" { (locale_options(ctx)) }
                    }
                }
                div.dropdown-divider {}
                button.dropdown-item.dropdown-item-danger type="button" data-api-action="/api/auth/logout" data-method="POST" {
                    (icon(icons::LOGOUT)) span { "Sign Out" }
                }
            }
        }
    }
}

fn header(ctx: &PageContext<'_>, brand_name: &str, logo_src: &str) -> Markup {
    let board_tools = ctx.can_edit_board && ctx.is_board_page;
    html! {
        header.topbar {
            div.header-content {
                div.header-zone.header-zone-left {
                    a.brand href="/" {
                        img.brand-logo src=(logo_src) alt=(brand_name);
                        span.brand-name { (brand_name) }
                    }
                    @if ctx.demo_mode {
                        span.demo-pill title=(if ctx.demo_read_only { "Demo Read-Only" } else { "Demo Mode" }) { "Demo" }
                    }
                    @if !ctx.boards.is_empty() { (board_switcher(ctx)) }
                }
                div.header-zone.header-zone-center {
                    button.spotlight-trigger type="button" onclick="window.openSpotlight()" aria-label="Search boards and apps" {
                        span.spotlight-trigger-inner {
                            (icon(icons::SEARCH))
                            span.spotlight-placeholder { "Search boards, apps, or web..." }
                        }
                        span.kbd { "Ctrl K" }
                    }
                }
                div.header-zone.header-zone-right {
                    @if board_tools {
                        button #edit-mode-btn .icon-btn type="button" onclick="window.toggleEditMode()" title="Edit Board" aria-label="Edit Board" { (icon(icons::PENCIL)) }
                        @if let Some(active) = ctx.active_board_name {
                            a.icon-btn href={ "/boards/" (urlencoding(active)) "/settings" } title="Board Settings" aria-label="Board Settings" { (icon(icons::SETTINGS)) }
                        }
                    }
                    button.icon-btn type="button" onclick="window.toggleTheme()" title="Toggle color scheme" aria-label="Toggle color scheme" { (icon(icons::SUN_MOON)) }
                    @if ctx.session.is_some_and(AuthSession::is_admin) {
                        a.icon-btn href="/manage" title="Manage Homarr" aria-label="Manage Homarr" { (icon(icons::SHIELD)) }
                    }
                    @if let Some(sess) = ctx.session {
                        (user_menu(ctx, sess))
                    } @else {
                        @if !ctx.i18n.codes().is_empty() {
                            div.locale-chip title="Language" {
                                (icon(icons::GLOBE))
                                select.locale-select aria-label="Language" onchange="window.setLocaleCookie(this.value)" { (locale_options(ctx)) }
                            }
                        }
                        a.btn.btn-primary.btn-sm href="/auth/login" { "Sign In" }
                    }
                }
            }
        }
    }
}

fn spotlight_modal() -> Markup {
    html! {
        div #spotlight-modal .modal-backdrop onclick="if(event.target===this)window.closeSpotlight()" {
            div.modal-card.spotlight-card {
                div.spotlight-input-row {
                    (icon(icons::SEARCH))
                    input #spotlight-input type="search" placeholder="Search boards, apps, pages, or actions..." autocomplete="off";
                    button.icon-btn.icon-btn-sm type="button" onclick="window.closeSpotlight()" aria-label="Close search" { (icon(icons::X)) }
                }
                div.spotlight-tabs role="tablist" aria-label="Filter results" {
                    button.spotlight-tab.is-active type="button" data-spotlight-cat="all" { "All" }
                    @for cat in ["Apps", "Boards", "Pages", "Actions", "Web"] {
                        button.spotlight-tab type="button" data-spotlight-cat=(cat) { (cat) }
                    }
                }
                div #spotlight-results .spotlight-results {}
                div.spotlight-footer {
                    span { kbd.kbd { "↑" } " " kbd.kbd { "↓" } " Navigate" }
                    span { kbd.kbd { "↵" } " Open" }
                    span { kbd.kbd { "Esc" } " Close" }
                }
            }
        }
    }
}

pub fn render_page(ctx: PageContext<'_>) -> String {
    let fallback_theme = match ctx.default_color_scheme {
        Some("light") => "light",
        _ => "dark",
    };
    let theme = ctx
        .session
        .map(|s| match s.color_scheme.as_str() {
            "light" => "light",
            "dark" => "dark",
            _ => fallback_theme,
        })
        .unwrap_or(fallback_theme);
    let brand_name = ctx
        .app_name
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Homarr");
    let robots = ctx.robots_meta.map(str::trim).filter(|s| !s.is_empty());

    let mut custom_style = String::new();
    if let Some(p) = sanitize_hex_color(ctx.primary_color) {
        custom_style.push_str(&format!(
            ":root{{--primary:{p};--primary-hover:color-mix(in srgb,{p} 82%,#000);}}"
        ));
    }
    if let Some(sec) = sanitize_hex_color(ctx.secondary_color) {
        custom_style.push_str(&format!(":root{{--secondary:{sec};--blue:{sec};}}"));
    }
    if let Some(op) = ctx.opacity {
        let frac = (op.clamp(0, 100) as f32) / 100.0;
        custom_style.push_str(&format!(":root{{--opacity:{frac:.2};}}"));
    }
    if let Some(rad) = map_item_radius(ctx.item_radius) {
        custom_style.push_str(&format!(":root{{--item-radius:{rad};}}"));
    }
    if let Some(css) = ctx.custom_css {
        custom_style.push_str(&sanitize_css(css));
    }

    let favicon_href =
        sanitize_css_url(ctx.favicon_image_url).unwrap_or_else(|| "/favicon.ico".to_string());
    let logo_src =
        sanitize_css_url(ctx.logo_image_url).unwrap_or_else(|| "/logo/homarr.svg".to_string());
    let background_style = sanitize_css_url(ctx.background_image_url).map(|bg_url| {
        format!(
            "background-image:url('{bg_url}');background-attachment:{};background-repeat:{};background-size:{}",
            sanitize_bg_attachment(ctx.background_image_attachment),
            sanitize_bg_repeat(ctx.background_image_repeat),
            sanitize_bg_size(ctx.background_image_size),
        )
    });

    let main_cls = if ctx.hide_header {
        "main-container main-auth"
    } else if ctx.is_board_page {
        "main-container board-canvas"
    } else {
        "main-container"
    };
    let body_cls = if ctx.background_image_url.is_some() {
        "has-board-bg"
    } else {
        ""
    };
    let doc_title = if ctx.title.trim() == brand_name {
        brand_name.to_string()
    } else {
        format!("{} — {brand_name}", ctx.title)
    };

    html! {
        (DOCTYPE)
        html lang=(ctx.locale) data-theme=(theme) {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                @if let Some(r) = robots { meta name="robots" content=(r); }
                title { (doc_title) }
                link rel="icon" href=(favicon_href);
                link rel="stylesheet" href="/static/homarr.css";
                @if !custom_style.is_empty() {
                    // Safe raw: colors/radius are validated and sanitize_css rejects `</style`.
                    style { (PreEscaped(&custom_style)) }
                }
            }
            body class=(body_cls) {
                @if let Some(style) = &background_style {
                    div.board-bg-layer style=(style) aria-hidden="true" {}
                }
                @if !ctx.hide_header { (header(&ctx, brand_name, &logo_src)) }
                main class=(main_cls) { (ctx.body) }
                (spotlight_modal())
                div #toast-stack .toast-stack {}
                (json_script("homarr-spotlight-data", &Value::Array(ctx.spotlight_items.to_vec())))
                script src="/static/homarr.js" defer {}
                @if ctx.can_edit_board {
                    script src="/static/board-editor.js" defer {}
                }
            }
        }
    }
    .into_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_page_escapes_title_and_spotlight_data() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/locales");
        let i18n = I18nCatalog::load(&dir);
        let items = [json!({ "label": "</script><script>alert(1)</script>" })];
        let ctx = PageContext {
            spotlight_items: &items,
            ..PageContext::minimal("<b>t</b>", html! { p { "ok" } }, "en", &i18n)
        };
        let out = render_page(ctx);
        assert!(out.contains("&lt;b&gt;t&lt;/b&gt;"));
        assert!(!out.contains("</script><script>alert"));
        assert!(out.contains(r#"<script type="application/json" id="homarr-spotlight-data">"#));
        assert!(out.contains("<main class=\"main-container\"><p>ok</p></main>"));
    }
}
