use axum::{
    extract::State,
    http::HeaderMap,
    response::{Html, IntoResponse},
};
use homarr_api::openapi::openapi_spec;
use homarr_auth::{AuthSession, OptionalAuth};
use homarr_db::repos::{boards, search_engines, settings};
use homarr_definitions::{
    integration::{all_integration_defs, IntegrationSecretKind},
    settings::parse_branding_settings,
};
use homarr_widgets::safe_url;
use maud::{html, Markup};
use serde_json::{json, Map, Value};

use super::manage::{empty_table_row, render_manage_shell, require_admin_or_redirect};
use crate::layout::{build_spotlight_items, render_page, PageContext, ServerUiSettings};
use crate::WebState;

fn secret_kind_label(kind: IntegrationSecretKind) -> &'static str {
    match kind {
        IntegrationSecretKind::ApiKey => "API Key",
        IntegrationSecretKind::Username => "Username",
        IntegrationSecretKind::Password => "Password",
        IntegrationSecretKind::TokenId => "Token ID",
        IntegrationSecretKind::Realm => "Realm",
        IntegrationSecretKind::PersonalAccessToken => "Personal Access Token",
        IntegrationSecretKind::Topic => "Topic",
        IntegrationSecretKind::OpnsenseApiKey => "OPNsense API Key",
        IntegrationSecretKind::OpnsenseApiSecret => "OPNsense API Secret",
        IntegrationSecretKind::PatchmonApiKey => "PatchMon API Key",
        IntegrationSecretKind::PatchmonApiSecret => "PatchMon API Secret",
        IntegrationSecretKind::Url => "Secondary URL",
        IntegrationSecretKind::PrivateKey => "Private Key",
        IntegrationSecretKind::GithubAppId => "GitHub App ID",
        IntegrationSecretKind::GithubInstallationId => "GitHub Installation ID",
        IntegrationSecretKind::Slug => "Slug",
        IntegrationSecretKind::WazuhIndexerUrl => "Wazuh Indexer URL",
        IntegrationSecretKind::WazuhIndexerUsername => "Wazuh Indexer Username",
        IntegrationSecretKind::WazuhIndexerPassword => "Wazuh Indexer Password",
        IntegrationSecretKind::TotpSecret => "TOTP Secret",
    }
}

/// Serializes all 92 integration definitions for the dynamic credential form in `/manage/integrations`.
pub(super) fn integration_defs_map_json() -> Value {
    let mut map = Map::new();
    for def in all_integration_defs() {
        let mut seen = Vec::new();
        let mut push_secret = |sk: IntegrationSecretKind, optional: bool| {
            let key = sk.as_str();
            if !seen.iter().any(|(k, _)| k == key) {
                seen.push((key.to_string(), (sk, optional)));
            }
        };
        for group in def.secret_kinds {
            for sk in *group {
                push_secret(*sk, false);
            }
        }
        for sk in def.optional_secret_kinds {
            push_secret(*sk, true);
        }
        let secrets: Vec<Value> = seen
            .into_iter()
            .map(|(k, (sk, opt))| {
                let meta = sk.metadata();
                json!({
                    "kind": k,
                    "label": secret_kind_label(sk),
                    "optional": opt,
                    "isPublic": meta.is_public,
                    "multiline": meta.multiline,
                })
            })
            .collect();
        let default_url = def.default_url.map(str::to_string).unwrap_or_else(|| {
            if let Some(p) = def.default_port {
                format!("http://127.0.0.1:{p}")
            } else {
                "https://service.local".to_string()
            }
        });
        map.insert(
            def.kind_str.to_string(),
            json!({
                "name": def.name,
                "defaultUrl": default_url,
                "secrets": secrets,
            }),
        );
    }
    Value::Object(map)
}

fn check_row(name: &str, label: &str, checked: bool, hint: &str) -> Markup {
    html! {
        label.setting-check {
            input type="checkbox" name=(name) style="width:auto" checked[checked];
            span {
                strong { (label) }
                " "
                @if !hint.is_empty() {
                    span.muted style="font-size:.76rem" { (hint) }
                }
            }
        }
    }
}

pub async fn settings_page(
    OptionalAuth(session): OptionalAuth,
    State(web): State<WebState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(redir) = require_admin_or_redirect(session.as_ref()) {
        return redir.into_response();
    }
    let all = settings::get_all_server_settings(&web.app.db)
        .await
        .unwrap_or_default();
    let board_list = boards::list_all(&web.app.db).await.unwrap_or_default();
    let engines = search_engines::list_all(&web.app.db)
        .await
        .unwrap_or_default();

    let board_s = all.get("board");
    let home_id = board_s
        .and_then(|v| v.get("homeBoardId"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let mob_id = board_s
        .and_then(|v| v.get("mobileHomeBoardId"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let status_def = board_s
        .and_then(|v| v.get("enableStatusByDefault"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let force_dis = board_s
        .and_then(|v| v.get("forceDisableStatus"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let board_opts = |sel: &str| -> Markup {
        html! {
            option value="" { "None (First visible board)" }
            @for b in board_list.iter().filter(|b| b.is_public) {
                option value=(b.id) selected[b.id == sel] { (b.name) }
            }
        }
    };

    let branding = all
        .get("branding")
        .map(parse_branding_settings)
        .unwrap_or_default();
    let def_scheme = all
        .get("appearance")
        .and_then(|v| v.get("defaultColorScheme"))
        .and_then(Value::as_str)
        .unwrap_or("dark");
    let def_se_id = all
        .get("search")
        .and_then(|v| v.get("defaultSearchEngineId"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let def_locale = all
        .get("culture")
        .and_then(|v| v.get("defaultLocale"))
        .and_then(Value::as_str)
        .unwrap_or("en");

    let gravatar = all
        .get("user")
        .and_then(|v| v.get("enableGravatar"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let analytics = all
        .get("analytics")
        .and_then(|v| v.get("enableGeneral"))
        .and_then(Value::as_bool)
        .unwrap_or(true);

    let crawl = all.get("crawlingAndIndexing");
    let no_index = crawl
        .and_then(|v| v.get("noIndex"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let no_follow = crawl
        .and_then(|v| v.get("noFollow"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let no_translate = crawl
        .and_then(|v| v.get("noTranslate"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let no_sitelinks = crawl
        .and_then(|v| v.get("noSiteLinksSearchBox"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let content = html! {
        div.page-header {
            div {
                h1.page-title { "Server Settings" }
                p.muted style="margin:4px 0 0" { "Configure instance-wide home boards, branding, localization, and crawler directives." }
            }
        }
        form.settings-stack data-settings-form="/api/settings" {
            div.card {
                h3.settings-section-title { "Boards & Status" }
                div.settings-two-col {
                    div.form-row { label { "Global Home Board" } select name="board.homeBoardId" { (board_opts(home_id)) } }
                    div.form-row { label { "Global Mobile Board" } select name="board.mobileHomeBoardId" { (board_opts(mob_id)) } }
                }
                div.settings-check-list {
                    (check_row("board.enableStatusByDefault", "Enable app status by default", status_def, "Ping new app items automatically"))
                    (check_row("board.forceDisableStatus", "Force disable app status pings", force_dis, "Disable status checks across all boards"))
                }
            }
            div.card {
                h3.settings-section-title { "Appearance & Branding" }
                div.settings-two-col {
                    div.form-row { label { "Application Name" } input name="branding.appName" value=(branding.app_name) maxlength="80" placeholder="Homarr"; }
                    div.form-row {
                        label { "Default Color Scheme" }
                        select name="appearance.defaultColorScheme" {
                            @for (val, lbl) in [("dark", "Dark"), ("light", "Light"), ("auto", "Auto (System)")] {
                                option value=(val) selected[val == def_scheme] { (lbl) }
                            }
                        }
                    }
                    div.form-row { label { "Logo Image URL" } input name="branding.logoImageUrl" value=(branding.logo_image_url.as_deref().unwrap_or("")) placeholder="/logo/homarr.svg"; }
                    div.form-row { label { "Favicon Image URL" } input name="branding.faviconImageUrl" value=(branding.favicon_image_url.as_deref().unwrap_or("")) placeholder="/favicon.ico"; }
                }
                div.form-row style="margin-top:10px" { label { "Login Page Greeting" } input name="branding.greeting" value=(branding.greeting) maxlength="160" placeholder="Welcome to your homelab workspace"; }
            }
            div.card {
                h3.settings-section-title { "Search & Localization" }
                div.settings-two-col {
                    div.form-row {
                        label { "Default Search Engine" }
                        select name="search.defaultSearchEngineId" {
                            option value="" { "None (DuckDuckGo fallback)" }
                            @for se in &engines {
                                option value=(se.id) selected[se.id == def_se_id] { (se.name) " (!" (se.short) ")" }
                            }
                        }
                    }
                    div.form-row {
                        label { "Default Language" }
                        select name="culture.defaultLocale" {
                            @for code in web.i18n.codes() {
                                option value=(code) selected[code == def_locale] { (code) }
                            }
                        }
                    }
                }
            }
            div.card {
                h3.settings-section-title { "Users & Analytics" }
                div.settings-check-list {
                    (check_row("user.enableGravatar", "Enable Gravatar fallback avatars", gravatar, ""))
                    (check_row("analytics.enableGeneral", "Enable general telemetry counter", analytics, ""))
                }
            }
            div.card {
                h3.settings-section-title { "Crawling & Indexing" }
                div.settings-check-list {
                    (check_row("crawlingAndIndexing.noIndex", "No index (noindex)", no_index, "Advise search engines not to index this instance"))
                    (check_row("crawlingAndIndexing.noFollow", "No follow (nofollow)", no_follow, ""))
                    (check_row("crawlingAndIndexing.noTranslate", "No translate (notranslate)", no_translate, ""))
                    (check_row("crawlingAndIndexing.noSiteLinksSearchBox", "Hide search box sitelinks", no_sitelinks, ""))
                }
            }
            div style="display:flex;justify-content:flex-end" {
                button.btn.btn-primary type="submit" { "Save Settings" }
            }
        }
    };
    render_manage_shell(
        &web,
        session.as_ref(),
        &headers,
        "/manage/settings",
        "Settings",
        content,
    )
    .await
    .into_response()
}

pub async fn search_engines_page(
    OptionalAuth(session): OptionalAuth,
    State(web): State<WebState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(redir) = require_admin_or_redirect(session.as_ref()) {
        return redir.into_response();
    }
    let engines = search_engines::list_all(&web.app.db)
        .await
        .unwrap_or_default();
    let content = html! {
        div.page-header { h1.page-title { "Search Engines" } }
        div.card {
            h3 style="margin-top:0" { "Add Search Engine" }
            form.form-grid style="max-width:none" data-api-form="/api/search-engines" {
                div.settings-two-col {
                    div.form-row { label { "Name" } input name="name" required placeholder="Startpage"; }
                    div.form-row { label { "Shortcut Prefix (without !)" } input name="short" required placeholder="sp"; }
                }
                div.settings-two-col {
                    div.form-row { label { "URL Template (%s placeholder)" } input name="urlTemplate" required placeholder="https://www.startpage.com/sp/search?q=%s"; }
                    div.form-row { label { "Icon URL" } input name="iconUrl" required value="/logo/homarr.svg"; }
                }
                div.form-row { label { "Description (optional)" } input name="description" placeholder="Private web search"; }
                div { button.btn.btn-primary type="submit" { "Add Search Engine" } }
            }
        }
        div.card {
            div.table-toolbar {
                input.table-search-input type="search" data-table-filter="#se-table" placeholder="Search engines..." aria-label="Filter search engines";
            }
            div.table-wrap {
                table #se-table {
                    thead { tr { th { "Engine" } th { "Prefix" } th { "URL Template" } th { "Actions" } } }
                    tbody {
                        @for se in &engines {
                            tr {
                                td {
                                    div.table-name-cell {
                                        img.table-app-icon src=(safe_url(&se.icon_url)) alt="" loading="lazy" onerror="this.onerror=null;this.src='/logo/homarr.svg'";
                                        span { strong { (se.name) } }
                                    }
                                }
                                td { code { "!" (se.short) } }
                                td { code { (se.url_template.as_deref().unwrap_or("-")) } }
                                td { button.btn.btn-danger.btn-sm type="button" data-api-action={ "/api/search-engines/" (se.id) } data-method="DELETE" { "Delete" } }
                            }
                        }
                        @if engines.is_empty() {
                            (empty_table_row(4, "No search engines configured yet."))
                        }
                    }
                }
            }
        }
    };
    render_manage_shell(
        &web,
        session.as_ref(),
        &headers,
        "/manage/search-engines",
        "Search Engines",
        content,
    )
    .await
    .into_response()
}

pub async fn api_reference_page(
    OptionalAuth(session): OptionalAuth,
    State(web): State<WebState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let spec = openapi_spec().await.0;
    let paths = spec.get("paths").and_then(Value::as_object);

    let content = html! {
        div.page-header {
            div {
                h1.page-title { "REST, WebSocket & MCP API Reference" }
                p.muted style="margin:4px 0 0" { "OpenAPI 3.1.0 specification, authentication headers, and live endpoints." }
            }
            div style="display:flex;gap:8px;flex-wrap:wrap" {
                a.btn.btn-sm href="/api/openapi" download="homarr-openapi.json" { "Download OpenAPI JSON" }
                a.btn.btn-sm href="/manage/tools/api-keys" { "Manage API Keys" }
            }
        }
        div.cards-grid {
            div.card style="margin-bottom:0" {
                h3 style="margin-top:0" { "Authentication" }
                p.muted style="font-size:.86rem;margin-bottom:8px" { "Authenticate via session cookie, API key header, or bearer token:" }
                div { code { "ApiKey: <id>.<secret>" } }
                div style="margin-top:6px" { code { "Authorization: Bearer <token>" } }
            }
            div.card style="margin-bottom:0" {
                h3 style="margin-top:0" { "MCP & WebSocket" }
                p.muted style="font-size:.86rem;margin-bottom:8px" { "Connect Model Context Protocol clients and live socket listeners:" }
                div { strong { "MCP:" } " " code { "POST /api/mcp" } " (JSON-RPC 2.0)" }
                div style="margin-top:6px" { strong { "WebSocket:" } " " code { "GET /websockets" } }
            }
        }
        div.card style="margin-top:16px" {
            div.table-toolbar style="justify-content:space-between;display:flex;align-items:center;gap:12px;flex-wrap:wrap" {
                h3 style="margin:0" { "OpenAPI 3.1 Endpoints" }
                input.table-search-input type="search" data-list-filter="#openapi-list" placeholder="Filter endpoints (e.g. /api/boards, POST)..." aria-label="Filter API endpoints";
            }
            div #openapi-list .api-ep-list {
                @for (path, methods) in paths.into_iter().flatten() {
                    @for (method, meta) in methods.as_object().into_iter().flatten() {
                        div.api-ep-row {
                            span class={ "http-badge http-" (method.to_ascii_lowercase()) } { (method.to_ascii_uppercase()) }
                            code.api-ep-path { (path) }
                            span.muted.api-ep-summary { (meta.get("summary").and_then(Value::as_str).unwrap_or("")) }
                        }
                    }
                }
            }
        }
    };

    if session.as_ref().is_some_and(AuthSession::is_admin) {
        return render_manage_shell(
            &web,
            session.as_ref(),
            &headers,
            "/reference/api",
            "API Reference",
            content,
        )
        .await
        .into_response();
    }

    let state = &web.app;
    let locale = web.i18n.negotiate(&headers);
    let visible = super::board::list_visible_boards(state, session.as_ref()).await;
    let spotlight = build_spotlight_items(state, &visible, false, false).await;
    let ui = ServerUiSettings::load(&state.db).await;
    let mut ctx = PageContext {
        session: session.as_ref(),
        demo_mode: state.config.demo_mode,
        demo_read_only: state.config.demo_read_only,
        boards: &visible,
        spotlight_items: &spotlight,
        ..PageContext::minimal("API Reference", content, &locale, &web.i18n)
    };
    ui.apply(&mut ctx);
    Html(render_page(ctx)).into_response()
}
