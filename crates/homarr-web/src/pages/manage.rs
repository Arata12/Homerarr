use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect},
};
use homarr_auth::{AuthSession, OptionalAuth};
use homarr_db::models::{AppRecord, AssistantConfigRecord};
use homarr_db::repos::{
    api_keys, apps, assistant, boards, custom_widgets, groups, integrations, invites,
    search_engines, users,
};
use homarr_definitions::{integration::all_integration_defs, WidgetKind};
use homarr_widgets::{all_widget_metadata, get_widget_metadata, render_widget_html, safe_url};
use maud::{html, Markup, PreEscaped};
use serde_json::json;

use super::board_editor::script_json;
use super::manage_extra::integration_defs_map_json;
pub use super::manage_extra::{api_reference_page, search_engines_page, settings_page};
use crate::icons::{self, icon};
use crate::layout::{build_spotlight_items, render_page, PageContext, ServerUiSettings};
use crate::WebState;

struct NavGroup {
    heading: &'static str,
    links: &'static [(&'static str, &'static str, &'static str)],
}

const NAV_GROUPS: &[NavGroup] = &[
    NavGroup {
        heading: "Workspace",
        links: &[
            ("/manage", "Overview", icons::HOME),
            ("/manage/boards", "Boards", icons::LAYOUT_DASHBOARD),
            ("/manage/apps", "Apps", icons::APPS),
            ("/manage/integrations", "Integrations", icons::PLUG),
            ("/manage/search-engines", "Search Engines", icons::SEARCH),
            ("/manage/custom-widgets", "Custom Widgets", icons::CODE),
            ("/manage/settings", "Settings", icons::SLIDERS),
        ],
    },
    NavGroup {
        heading: "Users & Access",
        links: &[
            ("/manage/users", "Users", icons::USERS),
            ("/manage/users/groups", "Groups", icons::USERS_GROUP),
            ("/manage/users/invites", "Invites", icons::MAIL),
        ],
    },
    NavGroup {
        heading: "Tools",
        links: &[
            ("/manage/tools/docker", "Docker", icons::DOCKER),
            ("/manage/tools/kubernetes", "Kubernetes", icons::SERVER),
            ("/manage/tools/tasks", "Tasks", icons::CLOCK),
            ("/manage/tools/logs", "Logs", icons::FILE_TEXT),
            (
                "/manage/tools/certificates",
                "Certificates",
                icons::CERTIFICATE,
            ),
            ("/manage/tools/api-keys", "API Keys", icons::KEY),
            ("/manage/tools/backup", "Backup", icons::DATABASE),
        ],
    },
    NavGroup {
        heading: "System",
        links: &[
            ("/manage/assistant", "Assistant", icons::ROBOT),
            ("/manage/about", "About", icons::INFO),
            ("/reference/api", "API Docs", icons::BOOK),
        ],
    },
];

pub(super) fn empty_table_row(colspan: usize, message: &str) -> Markup {
    html! {
        tr { td colspan=(colspan) class="muted" style="text-align:center;padding:24px" { (message) } }
    }
}

/// Wraps a `tbody` body, falling back to an empty-state row when no rows were rendered.
fn rows_or_empty(rows: Markup, colspan: usize, message: &str) -> Markup {
    if rows.0.is_empty() {
        empty_table_row(colspan, message)
    } else {
        rows
    }
}

pub(super) async fn render_manage_shell(
    web: &WebState,
    session: Option<&AuthSession>,
    headers: &HeaderMap,
    active_path: &str,
    title: &str,
    content: Markup,
) -> Html<String> {
    let state = &web.app;
    let locale = web.i18n.negotiate(headers);
    let visible_boards = super::board::list_visible_boards(state, session).await;
    let is_admin = session.is_some_and(AuthSession::is_admin);
    let spotlight = build_spotlight_items(state, &visible_boards, is_admin, false).await;
    let ui = ServerUiSettings::load(&state.db).await;

    let full_body = html! {
        div.manage-mobile-bar {
            button.btn.btn-sm type="button" onclick="window.toggleManageDrawer()" { (icon(icons::MENU)) span { "Menu" } }
            span.muted style="font-size:.84rem" { (title) }
        }
        div.manage-drawer-backdrop onclick="window.closeManageDrawer()" {}
        div.manage-shell {
            aside.sidebar #manage-sidebar {
                div.sidebar-mobile-head {
                    strong { "Navigation" }
                    button.icon-btn.icon-btn-sm type="button" onclick="window.closeManageDrawer()" aria-label="Close menu" { (icon(icons::X)) }
                }
                nav.sidebar-nav {
                    @for group in NAV_GROUPS {
                        div.sidebar-group-title { (group.heading) }
                        @for (href, label, ic) in group.links {
                            a.sidebar-link.active[*href == active_path] href=(href) { (icon(ic)) span { (label) } }
                        }
                    }
                }
            }
            div.manage-content { (content) }
        }
    };

    let mut ctx = PageContext {
        session,
        demo_mode: state.config.demo_mode,
        demo_read_only: state.config.demo_read_only,
        boards: &visible_boards,
        spotlight_items: &spotlight,
        ..PageContext::minimal(title, full_body, &locale, &web.i18n)
    };
    ui.apply(&mut ctx);
    Html(render_page(ctx))
}

pub(super) fn require_admin_or_redirect(
    session: Option<&AuthSession>,
) -> Result<&AuthSession, Redirect> {
    match session {
        Some(s) if s.is_admin() => Ok(s),
        _ => Err(Redirect::temporary("/auth/login")),
    }
}

pub async fn overview(
    OptionalAuth(session): OptionalAuth,
    State(web): State<WebState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let sess = match require_admin_or_redirect(session.as_ref()) {
        Ok(s) => s,
        Err(redir) => return redir.into_response(),
    };
    let state = &web.app;
    let boards_cnt = boards::list_all(&state.db)
        .await
        .map(|v| v.len())
        .unwrap_or(0);
    let apps_cnt = apps::list_all(&state.db)
        .await
        .map(|v| v.len())
        .unwrap_or(0);
    let ints_cnt = integrations::list_all(&state.db)
        .await
        .map(|v| v.len())
        .unwrap_or(0);
    let users_cnt = usize::try_from(users::count(&state.db).await.unwrap_or(0)).unwrap_or(0);
    let groups_cnt = groups::list_all(&state.db)
        .await
        .map(|v| v.len())
        .unwrap_or(0);
    let invites_cnt = invites::list_all(&state.db)
        .await
        .map(|v| v.len())
        .unwrap_or(0);
    let se_cnt = search_engines::list_all(&state.db)
        .await
        .map(|v| v.len())
        .unwrap_or(0);
    let cw_cnt = custom_widgets::list_v2(&state.db)
        .await
        .map(|v| v.len())
        .unwrap_or(0);

    let uname = sess.username.as_deref().unwrap_or("admin");
    let kpis = [
        (
            "/manage/boards",
            "Boards",
            "Boards",
            boards_cnt,
            "kpi-red",
            icons::LAYOUT_DASHBOARD,
        ),
        (
            "/manage/apps",
            "Apps",
            "Resources",
            apps_cnt,
            "kpi-blue",
            icons::APPS,
        ),
        (
            "/manage/integrations",
            "Integrations",
            "Resources",
            ints_cnt,
            "kpi-green",
            icons::PLUG,
        ),
        (
            "/manage/users",
            "Users",
            "Authentication",
            users_cnt,
            "kpi-yellow",
            icons::USERS,
        ),
        (
            "/manage/users/groups",
            "Groups",
            "Authorization",
            groups_cnt,
            "kpi-blue",
            icons::USERS_GROUP,
        ),
        (
            "/manage/users/invites",
            "Invites",
            "Authentication",
            invites_cnt,
            "kpi-red",
            icons::MAIL,
        ),
        (
            "/manage/search-engines",
            "Search Engines",
            "Resources",
            se_cnt,
            "kpi-green",
            icons::SEARCH,
        ),
        (
            "/manage/custom-widgets",
            "Custom Widgets",
            "Widgets",
            cw_cnt,
            "kpi-yellow",
            icons::CODE,
        ),
    ];

    let content = html! {
        div.card.manage-hero {
            div style="display:flex;align-items:center;justify-content:space-between;gap:16px;flex-wrap:wrap" {
                div {
                    h1.page-title style="font-size:1.7rem" {
                        "Welcome back, " span style="color:var(--primary)" { (uname) }
                    }
                    p.muted style="margin:6px 0 0" { "Everything you manage in Homarr, in one place." }
                }
                div style="display:flex;gap:8px;flex-wrap:wrap" {
                    a.btn.btn-primary href="/manage/boards" { (icon(icons::PLUS)) span { "Create Board" } }
                    a.btn href="/manage/apps" { (icon(icons::APPS)) span { "Add App" } }
                    a.btn href="/manage/integrations" { (icon(icons::PLUG)) span { "Add Integration" } }
                    a.btn href="/manage/settings" { (icon(icons::SLIDERS)) span { "Settings" } }
                }
            }
        }
        div.cards-grid {
            @for (href, label, sub, count, color_cls, ic) in kpis {
                a.card.kpi-card href=(href) {
                    div class={ "kpi-icon " (color_cls) } { (icon(ic)) }
                    div style="flex:1;min-width:0" {
                        div.metric-big { (count) }
                        div style="font-weight:600;color:var(--text-strong);font-size:.9rem" { (label) }
                        div.muted style="font-size:.74rem" { (sub) }
                    }
                    span.kpi-arrow { (icon(icons::ARROW_UP_RIGHT)) }
                }
            }
        }
        div.card {
            h3 style="margin-top:0" { "Tools & System" }
            div style="display:flex;gap:10px;flex-wrap:wrap" {
                a.btn.btn-sm href="/manage/tools/tasks" { (icon(icons::CLOCK)) span { "Cron Tasks" } }
                a.btn.btn-sm href="/manage/tools/docker" { (icon(icons::DOCKER)) span { "Docker" } }
                a.btn.btn-sm href="/manage/tools/logs" { (icon(icons::FILE_TEXT)) span { "Server Logs" } }
                a.btn.btn-sm href="/manage/tools/api-keys" { (icon(icons::KEY)) span { "API Keys" } }
                a.btn.btn-sm href="/manage/tools/backup" { (icon(icons::DATABASE)) span { "Backup" } }
                a.btn.btn-sm href="/manage/assistant" { (icon(icons::SPARKLES)) span { "AI Assistant" } }
                a.btn.btn-sm href="/reference/api" { (icon(icons::BOOK)) span { "API Docs" } }
            }
        }
    };
    render_manage_shell(
        &web,
        session.as_ref(),
        &headers,
        "/manage",
        "Manage",
        content,
    )
    .await
    .into_response()
}

pub async fn boards_page(
    OptionalAuth(session): OptionalAuth,
    State(web): State<WebState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(redir) = require_admin_or_redirect(session.as_ref()) {
        return redir.into_response();
    }
    let list = boards::list_all(&web.app.db).await.unwrap_or_default();
    let rows = html! {
        @for b in &list {
            tr {
                td { a href={ "/boards/" (b.name) } { strong { (b.name) } } }
                td { span.badge { (if b.is_public { "Public" } else { "Private" }) } }
                td style="display:flex;gap:6px" {
                    a.btn.btn-sm href={ "/boards/" (b.name) } { (icon(icons::EXTERNAL_LINK)) span { "Open" } }
                    a.btn.btn-sm href={ "/boards/" (b.name) "/settings" } { "Settings" }
                    button.btn.btn-danger.btn-sm type="button" data-api-action={ "/api/boards/" (b.id) } data-method="DELETE" { "Delete" }
                }
            }
        }
    };
    let content = html! {
        div.page-header { h1.page-title { "Boards" } }
        div.card {
            h3 style="margin-top:0" { "Create New Board" }
            form.form-grid data-api-form="/api/boards" {
                div.form-row { label { "Name" } input name="name" required placeholder="Homelab"; }
                label style="display:flex;align-items:center;gap:8px" {
                    input type="checkbox" name="isPublic" style="width:auto" checked;
                    " Public"
                }
                button.btn.btn-primary type="submit" { "Create Board" }
            }
        }
        div.card.table-wrap {
            table {
                thead { tr { th { "Name" } th { "Visibility" } th { "Actions" } } }
                tbody { (rows_or_empty(rows, 3, "No boards yet. Create your first board above.")) }
            }
        }
    };
    render_manage_shell(
        &web,
        session.as_ref(),
        &headers,
        "/manage/boards",
        "Manage Boards",
        content,
    )
    .await
    .into_response()
}

fn app_row(a: &AppRecord) -> Markup {
    let href_safe = a.href.as_deref().map(safe_url).filter(|u| *u != "#");
    let app_payload = json!({
        "id": a.id,
        "name": a.name,
        "description": a.description,
        "iconUrl": a.icon_url,
        "href": a.href,
        "pingUrl": a.ping_url,
    })
    .to_string();
    html! {
        tr {
            td {
                div.table-name-cell {
                    img.table-app-icon src=(safe_url(&a.icon_url)) alt="" loading="lazy" onerror="this.onerror=null;this.src='/logo/homarr.svg'";
                    div {
                        strong { (a.name) }
                        div.muted style="font-size:.76rem" { (a.description.as_deref().unwrap_or("")) }
                    }
                }
            }
            td {
                @if let Some(u) = href_safe {
                    a href=(u) target="_blank" rel="noopener noreferrer" { code { (u) } }
                } @else {
                    span.muted { "-" }
                }
            }
            td style="display:flex;gap:6px" {
                button.btn.btn-sm type="button" data-edit-app=(app_payload) { "Edit" }
                button.btn.btn-danger.btn-sm type="button" data-api-action={ "/api/apps/" (a.id) } data-method="DELETE" { "Delete" }
            }
        }
    }
}

pub async fn apps_page(
    OptionalAuth(session): OptionalAuth,
    State(web): State<WebState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(redir) = require_admin_or_redirect(session.as_ref()) {
        return redir.into_response();
    }
    let list = apps::list_all(&web.app.db).await.unwrap_or_default();
    let rows = html! { @for a in &list { (app_row(a)) } };
    let content = html! {
        div.page-header { h1.page-title { "Applications" } }
        div.card {
            h3 style="margin-top:0" { "Add Application" }
            form.form-grid style="max-width:none" data-api-form="/api/apps" {
                div.settings-two-col {
                    div.form-row { label { "Name" } input name="name" required placeholder="Sonarr"; }
                    div.form-row { label { "URL" } input name="href" placeholder="https://sonarr.local"; }
                    div.form-row { label { "Icon URL" } input #app-icon-url-input name="iconUrl" required value="/logo/homarr.svg"; }
                    div.form-row { label { "Description" } input name="description" placeholder="Optional description"; }
                }
                div.form-row {
                    label { "Search Bundled Icons" }
                    input type="search" data-icon-search="#app-icon-picker" data-icon-target="#app-icon-url-input" placeholder="Type e.g. sonarr, grafana, plex, nextcloud..." autocomplete="off";
                    div #app-icon-picker .editor-icon-results {}
                }
                div { button.btn.btn-primary type="submit" { "Add Application" } }
            }
        }
        div.card {
            div.table-toolbar {
                input.table-search-input type="search" data-table-filter="#apps-table" placeholder="Search applications..." aria-label="Filter applications";
            }
            div.table-wrap {
                table #apps-table {
                    thead { tr { th { "Application" } th { "URL" } th { "Actions" } } }
                    tbody { (rows_or_empty(rows, 3, "No applications yet. Add your first application above.")) }
                }
            }
        }
    };
    render_manage_shell(
        &web,
        session.as_ref(),
        &headers,
        "/manage/apps",
        "Manage Apps",
        content,
    )
    .await
    .into_response()
}

pub async fn integrations_page(
    OptionalAuth(session): OptionalAuth,
    State(web): State<WebState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(redir) = require_admin_or_redirect(session.as_ref()) {
        return redir.into_response();
    }
    let list = integrations::list_all(&web.app.db)
        .await
        .unwrap_or_default();
    let rows = html! {
        @for int in &list {
            tr {
                td { strong { (int.name) } }
                td { span.badge { (int.kind) } }
                td { code { (int.url) } }
                td style="display:flex;gap:6px" {
                    button.btn.btn-sm type="button" data-api-action={ "/api/integrations/" (int.id) "/test" } data-method="POST" data-reload="false" { "Test" }
                    button.btn.btn-danger.btn-sm type="button" data-api-action={ "/api/integrations/" (int.id) } data-method="DELETE" { "Delete" }
                }
            }
        }
    };
    let defs_json = script_json(&integration_defs_map_json());
    let content = html! {
        div.page-header { h1.page-title { "Integrations" } }
        div.card {
            h3 style="margin-top:0" { "Connect Integration" }
            form #integration-create-form .form-grid style="max-width:none" data-api-form="/api/integrations" {
                div.settings-two-col {
                    div.form-row {
                        label { "Kind (92 Supported)" }
                        select #integration-kind-select name="kind" {
                            @for def in all_integration_defs() {
                                option value=(def.kind_str) { (def.name) }
                            }
                        }
                    }
                    div.form-row { label { "Name" } input name="name" required placeholder="My Service"; }
                }
                div.form-row { label { "URL" } input #integration-url-input name="url" required placeholder="http://127.0.0.1:8989"; }
                div #integration-secret-fields .settings-two-col {}
                details.editor-details {
                    summary { "Advanced: Raw Secrets JSON" }
                    div.form-row style="margin-top:8px" { input name="secretsJson" placeholder="[{\"kind\":\"apiKey\",\"value\":\"...\"}]"; }
                }
                div { button.btn.btn-primary type="submit" { "Save Integration" } }
            }
        }
        div.card {
            div.table-toolbar {
                input.table-search-input type="search" data-table-filter="#integrations-table" placeholder="Search integrations..." aria-label="Filter integrations";
            }
            div.table-wrap {
                table #integrations-table {
                    thead { tr { th { "Name" } th { "Kind" } th { "URL" } th { "Actions" } } }
                    tbody { (rows_or_empty(rows, 4, "No integrations connected yet.")) }
                }
            }
        }
        // safe: script_json escapes <, >, & and line separators, so the JSON cannot close the script tag.
        script { (PreEscaped(format!("window.__HOMARR_INTEGRATION_DEFS__ = {defs_json};"))) }
    };
    render_manage_shell(
        &web,
        session.as_ref(),
        &headers,
        "/manage/integrations",
        "Integrations",
        content,
    )
    .await
    .into_response()
}

pub async fn users_page(
    OptionalAuth(session): OptionalAuth,
    State(web): State<WebState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(redir) = require_admin_or_redirect(session.as_ref()) {
        return redir.into_response();
    }
    let list = users::list_all(&web.app.db).await.unwrap_or_default();
    let rows = html! {
        @for u in &list {
            tr {
                td { strong { (u.name.as_deref().unwrap_or("")) } }
                td { (u.email.as_deref().unwrap_or("-")) }
                td { span.badge { (u.provider) } }
                td { button.btn.btn-danger.btn-sm type="button" data-api-action={ "/api/users/" (u.id) } data-method="DELETE" { "Delete" } }
            }
        }
    };
    let content = html! {
        div.page-header { h1.page-title { "Users" } }
        div.card {
            h3 style="margin-top:0" { "Create User" }
            form.form-grid style="max-width:none" data-api-form="/api/users" {
                div.settings-two-col {
                    div.form-row { label { "Username" } input name="name" required minlength="2"; }
                    div.form-row { label { "Email" } input name="email" type="email"; }
                }
                div.form-row { label { "Password" } input name="password" type="password" required minlength="8"; }
                div { button.btn.btn-primary type="submit" { "Create User" } }
            }
        }
        div.card {
            div.table-toolbar {
                input.table-search-input type="search" data-table-filter="#users-table" placeholder="Search users..." aria-label="Filter users";
            }
            div.table-wrap {
                table #users-table {
                    thead { tr { th { "Username" } th { "Email" } th { "Provider" } th { "Actions" } } }
                    tbody { (rows_or_empty(rows, 4, "No users found.")) }
                }
            }
        }
    };
    render_manage_shell(
        &web,
        session.as_ref(),
        &headers,
        "/manage/users",
        "Users",
        content,
    )
    .await
    .into_response()
}

pub async fn groups_page(
    OptionalAuth(session): OptionalAuth,
    State(web): State<WebState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(redir) = require_admin_or_redirect(session.as_ref()) {
        return redir.into_response();
    }
    let list = groups::list_all(&web.app.db).await.unwrap_or_default();
    let mut group_perms = Vec::with_capacity(list.len());
    for g in &list {
        let perms = groups::get_group_permissions(&web.app.db, &g.id)
            .await
            .unwrap_or_default()
            .join(", ");
        group_perms.push((g, perms));
    }
    let rows = html! {
        @for (g, perms) in &group_perms {
            tr {
                td { strong { (g.name) } }
                td { code { (perms) } }
                td { button.btn.btn-danger.btn-sm type="button" data-api-action={ "/api/groups/" (g.id) } data-method="DELETE" { "Delete" } }
            }
        }
    };
    let content = html! {
        div.page-header { h1.page-title { "Groups" } }
        div.card {
            h3 style="margin-top:0" { "Create Group" }
            form.form-grid data-api-form="/api/groups" {
                div.form-row { label { "Group Name" } input name="name" required; }
                button.btn.btn-primary type="submit" { "Create Group" }
            }
        }
        div.card.table-wrap {
            table {
                thead { tr { th { "Name" } th { "Permissions" } th { "Actions" } } }
                tbody { (rows_or_empty(rows, 3, "No groups found.")) }
            }
        }
    };
    render_manage_shell(
        &web,
        session.as_ref(),
        &headers,
        "/manage/users/groups",
        "Groups",
        content,
    )
    .await
    .into_response()
}

pub async fn invites_page(
    OptionalAuth(session): OptionalAuth,
    State(web): State<WebState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(redir) = require_admin_or_redirect(session.as_ref()) {
        return redir.into_response();
    }
    let list = invites::list_all(&web.app.db).await.unwrap_or_default();
    let rows = html! {
        @for inv in &list {
            tr {
                td { code { "/auth/invite/" (inv.token) } }
                td { (inv.expiration_date) }
                td { button.btn.btn-danger.btn-sm type="button" data-api-action={ "/api/invites/" (inv.id) } data-method="DELETE" { "Revoke" } }
            }
        }
    };
    let content = html! {
        div.page-header { h1.page-title { "Invites" } }
        div.card { button.btn.btn-primary type="button" data-api-action="/api/invites" data-method="POST" { "Generate Invite Link" } }
        div.card.table-wrap {
            table {
                thead { tr { th { "Invite Path" } th { "Expires (Unix)" } th { "Actions" } } }
                tbody { (rows_or_empty(rows, 3, "No active invites.")) }
            }
        }
    };
    render_manage_shell(
        &web,
        session.as_ref(),
        &headers,
        "/manage/users/invites",
        "Invites",
        content,
    )
    .await
    .into_response()
}

pub async fn custom_widgets_page(
    OptionalAuth(session): OptionalAuth,
    State(web): State<WebState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(redir) = require_admin_or_redirect(session.as_ref()) {
        return redir.into_response();
    }
    let list = custom_widgets::list_v2(&web.app.db)
        .await
        .unwrap_or_default();
    let rows = html! {
        @for cw in &list {
            tr {
                td { strong { (cw.name) } }
                td { span.badge { (if cw.enabled { "Enabled" } else { "Disabled" }) } }
                td { button.btn.btn-danger.btn-sm type="button" data-api-action={ "/api/custom-widgets/" (cw.id) } data-method="DELETE" { "Delete" } }
            }
        }
    };
    let content = html! {
        div.page-header { h1.page-title { "Custom Widgets v2" } }
        div.card {
            h3 style="margin-top:0" { "Create Custom Widget" }
            form.form-grid data-api-form="/api/custom-widgets" {
                div.form-row { label { "Name" } input name="name" required placeholder="My Status Card"; }
                div.form-row {
                    label { "Template HTML (XSS-escaped placeholders)" }
                    textarea name="template" rows="4" required { "<div>{title}</div>" }
                }
                button.btn.btn-primary type="submit" { "Save Custom Widget" }
            }
        }
        div.card.table-wrap {
            table {
                thead { tr { th { "Name" } th { "Status" } th { "Actions" } } }
                tbody { (rows_or_empty(rows, 3, "No custom widgets created yet.")) }
            }
        }
    };
    render_manage_shell(
        &web,
        session.as_ref(),
        &headers,
        "/manage/custom-widgets",
        "Custom Widgets",
        content,
    )
    .await
    .into_response()
}

pub async fn docker_page(
    OptionalAuth(session): OptionalAuth,
    State(web): State<WebState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(redir) = require_admin_or_redirect(session.as_ref()) {
        return redir.into_response();
    }
    let containers = web
        .app
        .services
        .docker
        .list_containers()
        .await
        .unwrap_or_default();
    let rows = html! {
        @for c in &containers {
            tr {
                td { strong { (c.name) } }
                td { code { (c.image) } }
                td { span.badge { (c.state) } }
                td style="display:flex;gap:4px" {
                    button.btn.btn-sm type="button" data-api-action={ "/api/docker/containers/" (c.id) "/restart" } data-method="POST" { "Restart" }
                    button.btn.btn-sm type="button" data-api-action={ "/api/docker/containers/" (c.id) "/stop" } data-method="POST" { "Stop" }
                }
            }
        }
    };
    let content = html! {
        div.page-header {
            h1.page-title { "Docker Containers" }
            button.btn.btn-sm type="button" onclick="location.reload()" { "Refresh" }
        }
        @if containers.is_empty() {
            div.card.notice-warn {
                "Docker socket " code { "/var/run/docker.sock" } " is unavailable or no containers were returned."
            }
        }
        div.card {
            div.table-toolbar {
                input.table-search-input type="search" data-table-filter="#docker-table" placeholder="Search containers..." aria-label="Filter containers";
            }
            div.table-wrap {
                table #docker-table {
                    thead { tr { th { "Name" } th { "Image" } th { "State" } th { "Actions" } } }
                    tbody {
                        (rows_or_empty(rows, 4, "No Docker containers reported (or Docker socket is unavailable in this environment)."))
                    }
                }
            }
        }
    };
    render_manage_shell(
        &web,
        session.as_ref(),
        &headers,
        "/manage/tools/docker",
        "Docker",
        content,
    )
    .await
    .into_response()
}

pub async fn kubernetes_page(
    OptionalAuth(session): OptionalAuth,
    State(web): State<WebState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(redir) = require_admin_or_redirect(session.as_ref()) {
        return redir.into_response();
    }
    let pods = if web.app.services.kubernetes.is_enabled() {
        let pods = web
            .app
            .services
            .kubernetes
            .get_resource("pods")
            .await
            .unwrap_or_else(|e| json!({ "error": e.to_string() }));
        Some(serde_json::to_string_pretty(&pods).unwrap_or_default())
    } else {
        None
    };
    let content = html! {
        div.page-header { h1.page-title { "Kubernetes" } }
        div.card {
            @if let Some(pods) = &pods {
                pre.generic-json { (pods) }
            } @else {
                p.muted {
                    "Kubernetes integration is disabled. Set " code { "ENABLE_KUBERNETES=true" } " inside a cluster pod to enable it."
                }
            }
        }
    };
    render_manage_shell(
        &web,
        session.as_ref(),
        &headers,
        "/manage/tools/kubernetes",
        "Kubernetes",
        content,
    )
    .await
    .into_response()
}

pub async fn tasks_page(
    OptionalAuth(session): OptionalAuth,
    State(web): State<WebState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(redir) = require_admin_or_redirect(session.as_ref()) {
        return redir.into_response();
    }
    let jobs = web.app.scheduler.list_jobs(false).await;
    let rows = html! {
        @for j in &jobs {
            @let (badge_cls, enabled_lbl, toggle_lbl, next_val) = if j.is_enabled {
                ("up", "Enabled", "Pause", "false")
            } else {
                ("down", "Paused", "Enable", "true")
            };
            tr {
                td {
                    strong { (j.name) }
                    div.muted style="font-size:.78rem" { (j.description) }
                }
                td { code { (j.cron_expression) } }
                td { span class={ "badge " (badge_cls) } { (enabled_lbl) } }
                td style="display:flex;gap:6px" {
                    @if !j.prevent_manual_execution {
                        button.btn.btn-sm type="button" data-api-action={ "/api/cron-jobs/" (j.name) "/trigger" } data-method="POST" { "Run Now" }
                    }
                    button.btn.btn-sm type="button" data-cron-toggle=(j.name) data-enabled=(next_val) { (toggle_lbl) }
                }
            }
        }
    };
    let content = html! {
        div.page-header {
            h1.page-title { "Background Cron Tasks" }
            button.btn.btn-sm type="button" onclick="location.reload()" { "Refresh" }
        }
        div.card.table-wrap {
            table {
                thead { tr { th { "Task" } th { "Schedule" } th { "Status" } th { "Action" } } }
                tbody { (rows_or_empty(rows, 4, "No scheduled jobs registered.")) }
            }
        }
    };
    render_manage_shell(
        &web,
        session.as_ref(),
        &headers,
        "/manage/tools/tasks",
        "Tasks",
        content,
    )
    .await
    .into_response()
}

pub async fn logs_page(
    OptionalAuth(session): OptionalAuth,
    State(web): State<WebState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(redir) = require_admin_or_redirect(session.as_ref()) {
        return redir.into_response();
    }
    let entries = web.app.logs.recent(150);
    let content = html! {
        div.page-header {
            h1.page-title { "Server Logs" }
            div style="display:flex;gap:8px;align-items:center;flex-wrap:wrap" {
                select #log-level-filter aria-label="Filter log level" style="width:auto" {
                    option value="all" { "All Levels" }
                    option value="info" { "Info" }
                    option value="warn" { "Warn" }
                    option value="error" { "Error" }
                }
                input #log-text-filter type="search" placeholder="Search logs..." style="width:220px" aria-label="Search logs";
                button.btn.btn-sm type="button" onclick="location.reload()" { "Refresh" }
            }
        }
        div.log-terminal #log-terminal {
            @for l in &entries {
                @let lvl_lower = l.level.to_ascii_lowercase();
                @let lvl_cls = match lvl_lower.as_str() {
                    "error" => "log-level-error",
                    "warn" | "warning" => "log-level-warn",
                    _ => "log-level-info",
                };
                div.log-line data-level=(lvl_lower) {
                    span.log-time { (l.timestamp) }
                    " "
                    span class={ "badge " (lvl_cls) } { (l.level) }
                    " "
                    span.log-msg { (l.message) }
                }
            }
            @if entries.is_empty() {
                div.muted style="padding:16px" { "No server log entries recorded yet." }
            }
        }
    };
    render_manage_shell(
        &web,
        session.as_ref(),
        &headers,
        "/manage/tools/logs",
        "Logs",
        content,
    )
    .await
    .into_response()
}

pub async fn certificates_page(
    OptionalAuth(session): OptionalAuth,
    State(web): State<WebState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(redir) = require_admin_or_redirect(session.as_ref()) {
        return redir.into_response();
    }
    let certs = web.app.certs.list().unwrap_or_default();
    let rows = html! {
        @for c in &certs {
            tr {
                td { (c.file_name) }
                td { button.btn.btn-danger.btn-sm type="button" data-api-action={ "/api/certificates/" (c.file_name) } data-method="DELETE" { "Delete" } }
            }
        }
    };
    let content = html! {
        div.page-header { h1.page-title { "Trusted Certificates" } }
        div.card {
            form.form-grid data-api-form="/api/certificates" {
                div.form-row { label { "Filename (.crt or .pem)" } input name="fileName" required placeholder="homelab-ca.crt"; }
                div.form-row {
                    label { "PEM Content" }
                    textarea name="content" rows="5" required placeholder="-----BEGIN CERTIFICATE-----" {}
                }
                button.btn.btn-primary type="submit" { "Upload Certificate" }
            }
        }
        div.card.table-wrap {
            table {
                thead { tr { th { "Filename" } th { "Action" } } }
                tbody { (rows_or_empty(rows, 2, "No custom trusted certificates uploaded.")) }
            }
        }
    };
    render_manage_shell(
        &web,
        session.as_ref(),
        &headers,
        "/manage/tools/certificates",
        "Certificates",
        content,
    )
    .await
    .into_response()
}

pub async fn api_keys_page(
    OptionalAuth(session): OptionalAuth,
    State(web): State<WebState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let sess = match require_admin_or_redirect(session.as_ref()) {
        Ok(s) => s,
        Err(redir) => return redir.into_response(),
    };
    let keys = api_keys::list_by_user(&web.app.db, &sess.user_id)
        .await
        .unwrap_or_default();
    let rows = html! {
        @for k in &keys {
            tr {
                td { code { (k.id) } }
                td { span.badge { (if k.read_only { "Read-Only" } else { "Read/Write" }) } }
                td { button.btn.btn-danger.btn-sm type="button" data-api-action={ "/api/api-keys/" (k.id) } data-method="DELETE" { "Revoke" } }
            }
        }
    };
    let content = html! {
        div.page-header { h1.page-title { "API Keys" } }
        div #api-key-reveal .card.api-key-banner hidden {
            h3 style="margin-top:0" { "New API Key Generated" }
            p.muted style="margin:4px 0 10px" { "Copy this token now. For security reasons, it will not be displayed again after closing." }
            div style="display:flex;gap:8px;align-items:center;flex-wrap:wrap" {
                code #api-key-value style="padding:8px 12px;font-size:.92rem;user-select:all" {}
                button #copy-api-key-btn .btn.btn-sm.btn-primary type="button" { (icon(icons::COPY)) span { "Copy Key" } }
                button.btn.btn-sm type="button" onclick="location.reload()" { "Done & Refresh" }
            }
        }
        div.card {
            form.form-grid data-api-form="/api/api-keys" data-reveal-key="#api-key-reveal" {
                label style="display:flex;align-items:center;gap:8px" {
                    input type="checkbox" name="readOnly" style="width:auto";
                    " Read-Only Key"
                }
                button.btn.btn-primary type="submit" { "Create API Key" }
            }
        }
        div.card.table-wrap {
            table #api-keys-table {
                thead { tr { th { "Key ID" } th { "Scope" } th { "Action" } } }
                tbody { (rows_or_empty(rows, 3, "No API keys created yet.")) }
            }
        }
    };
    render_manage_shell(
        &web,
        session.as_ref(),
        &headers,
        "/manage/tools/api-keys",
        "API Keys",
        content,
    )
    .await
    .into_response()
}

pub async fn backup_page(
    OptionalAuth(session): OptionalAuth,
    State(web): State<WebState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(redir) = require_admin_or_redirect(session.as_ref()) {
        return redir.into_response();
    }
    let content = html! {
        div.page-header { h1.page-title { "Backup & Restore" } }
        div.card {
            h3 style="margin-top:0" { "Export Full Backup Bundle" }
            p.muted { "Downloads a portable JSON backup containing boards, apps, search engines, groups, and settings." }
            a.btn.btn-primary href="/api/backup/export" download="homarr-backup.json" { "Download Backup JSON" }
        }
        div.card {
            h3 style="margin-top:0" { "Restore from Backup JSON" }
            p.muted {
                "Select an exported " code { "homarr-backup.json" } " file or paste JSON below to restore workspace data."
            }
            div.form-grid style="max-width:none" {
                div.form-row { label { "Upload Backup File (.json)" } input #backup-file-input type="file" accept=".json,application/json"; }
                div.form-row {
                    label { "Backup JSON Payload" }
                    textarea #backup-json-input rows="6" placeholder="{\"boards\": [...], \"apps\": [...]}" {}
                }
                div { button.btn.btn-primary type="button" data-restore-backup { "Restore Backup" } }
            }
        }
    };
    render_manage_shell(
        &web,
        session.as_ref(),
        &headers,
        "/manage/tools/backup",
        "Backup",
        content,
    )
    .await
    .into_response()
}

fn assistant_form(cfg: &AssistantConfigRecord) -> Markup {
    html! {
        div.page-header { h1.page-title { "AI Assistant" } }
        div.card {
            form.form-grid data-api-form="/api/assistant/config" data-method="PUT" {
                label style="display:flex;align-items:center;gap:8px" {
                    input type="checkbox" name="enabled" style="width:auto" checked[cfg.enabled];
                    " Enable Assistant"
                }
                div.form-row { label { "Provider (openrouter, openai, anthropic, ollama)" } input name="provider" value=(cfg.provider) required; }
                div.form-row { label { "Base URL" } input name="baseUrl" value=(cfg.base_url) required; }
                div.form-row { label { "Model ID" } input name="modelId" value=(cfg.model_id.as_deref().unwrap_or("openai/gpt-4o-mini")); }
                div.form-row { label { "API Key (leave blank to keep existing)" } input name="apiKey" type="password"; }
                button.btn.btn-primary type="submit" { "Save Assistant Configuration" }
            }
        }
    }
}

pub async fn assistant_page(
    OptionalAuth(session): OptionalAuth,
    State(web): State<WebState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(redir) = require_admin_or_redirect(session.as_ref()) {
        return redir.into_response();
    }
    let cfg = assistant::get_config(&web.app.db)
        .await
        .unwrap_or_else(|_| AssistantConfigRecord {
            id: "default".into(),
            enabled: false,
            web_search_enabled: false,
            provider: "openrouter".into(),
            base_url: "https://openrouter.ai/api/v1".into(),
            model_discovery_path: None,
            encrypted_api_key: None,
            encrypted_headers: None,
            model_id: None,
            updated_at: 0,
        });
    render_manage_shell(
        &web,
        session.as_ref(),
        &headers,
        "/manage/assistant",
        "Assistant",
        assistant_form(&cfg),
    )
    .await
    .into_response()
}

pub async fn about_page(
    OptionalAuth(session): OptionalAuth,
    State(web): State<WebState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(redir) = require_admin_or_redirect(session.as_ref()) {
        return redir.into_response();
    }
    let content = html! {
        div.card.about-hero {
            img src="/logo/homarr.svg" alt="Homarr" style="width:64px;height:64px";
            div {
                h1.page-title { "Homarr " span.demo-pill { "v" (env!("CARGO_PKG_VERSION")) } }
                p.muted style="margin:6px 0 0" { "Static Pure-Rust Dashboard Server (Axum + SQLx + Tokio + Bollard)" }
            }
        }
        div.cards-grid {
            div.card style="margin-bottom:0" {
                h3 style="margin-top:0" { "Runtime & API" }
                div style="display:flex;gap:8px;flex-wrap:wrap;margin-bottom:10px" {
                    span.badge { "Distroless musl" }
                    " "
                    span.badge { "SQLite / PostgreSQL" }
                    " "
                    span.badge { "92 Integrations" }
                    " "
                    span.badge { "67 Widgets" }
                }
                p.muted style="font-size:.86rem;margin:0" {
                    "OpenAPI 3.1 spec at " a href="/api/openapi" { code { "/api/openapi" } }
                    " and MCP JSON-RPC 2.0 at " code { "POST /api/mcp" } "."
                }
            }
            div.card style="margin-bottom:0" {
                h3 style="margin-top:0" { "Resources" }
                div style="display:flex;gap:8px;flex-wrap:wrap" {
                    a.btn.btn-sm href="/reference/api" { "Interactive API Reference" }
                    a.btn.btn-sm href="https://homarr.dev/docs/getting-started" target="_blank" rel="noopener noreferrer" { "Documentation" }
                    a.btn.btn-sm href="https://github.com/homarr-labs/homarr" target="_blank" rel="noopener noreferrer" { "GitHub" }
                }
            }
        }
        div.card.table-wrap style="margin-top:16px" {
            h3 style="margin-top:0" { "Keyboard Shortcuts" }
            table {
                thead { tr { th { "Shortcut" } th { "Action" } } }
                tbody {
                    tr {
                        td {
                            kbd.kbd { "Ctrl" } " + " kbd.kbd { "K" } " / " kbd.kbd { "Cmd" } " + " kbd.kbd { "K" }
                        }
                        td { "Open Spotlight Search" }
                    }
                    tr { td { kbd.kbd { "Esc" } } td { "Close modal, Spotlight, or mobile menu drawer" } }
                    tr {
                        td { kbd.kbd { "↑" } " / " kbd.kbd { "↓" } " / " kbd.kbd { "↵" } }
                        td { "Navigate and open Spotlight results" }
                    }
                }
            }
        }
    };
    render_manage_shell(
        &web,
        session.as_ref(),
        &headers,
        "/manage/about",
        "About",
        content,
    )
    .await
    .into_response()
}

pub async fn widget_preview_page(
    OptionalAuth(session): OptionalAuth,
    State(web): State<WebState>,
    Path(kind_str): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let state = &web.app;
    let Some(kind) = WidgetKind::from_str_opt(&kind_str) else {
        return (StatusCode::NOT_FOUND, "Unknown widget kind").into_response();
    };
    let meta = get_widget_metadata(kind);
    let data = state
        .widgets
        .resolve_widget_data(kind, &meta.default_options, &[])
        .await
        .unwrap_or_else(|_| json!({}));
    let card = render_widget_html(kind, "preview", &meta.default_options, &data);

    let body = html! {
        div.page-header { h1.page-title { "Widget Preview: " (meta.name) } }
        // safe: render_widget_html escapes all widget data and options itself (tested in homarr-widgets).
        div.grid-item.widget-preview-box { (PreEscaped(card)) }
        div.card style="margin-top:16px" {
            h3 style="margin-top:0" { "All 67 Widgets" }
            div style="display:flex;gap:6px;flex-wrap:wrap" {
                @for m in all_widget_metadata() {
                    a.badge href={ "/widgets/" (m.kind) } { (m.name) }
                    " "
                }
            }
        }
    };

    let locale = web.i18n.negotiate(&headers);
    let ui = ServerUiSettings::load(&state.db).await;
    let visible = super::board::list_visible_boards(state, session.as_ref()).await;
    let is_admin = session.as_ref().is_some_and(AuthSession::is_admin);
    let spotlight = build_spotlight_items(state, &visible, is_admin, false).await;
    let mut ctx = PageContext {
        session: session.as_ref(),
        demo_mode: state.config.demo_mode,
        demo_read_only: state.config.demo_read_only,
        boards: &visible,
        spotlight_items: &spotlight,
        ..PageContext::minimal(&meta.name, body, &locale, &web.i18n)
    };
    ui.apply(&mut ctx);
    Html(render_page(ctx)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_table_row_escapes_message() {
        let out = empty_table_row(3, "<b>x</b>").into_string();
        assert!(out.contains(r#"colspan="3""#));
        assert!(out.contains("&lt;b&gt;x&lt;/b&gt;"));
    }

    #[test]
    fn app_row_escapes_attributes_and_blocks_unsafe_urls() {
        let app = AppRecord {
            id: "a\"1".into(),
            name: "<script>n</script>".into(),
            description: Some("\"><img>".into()),
            icon_url: "javascript:alert(1)".into(),
            href: Some("javascript:alert(2)".into()),
            ping_url: None,
        };
        let out = app_row(&app).into_string();
        assert!(!out.contains("<script>n"));
        assert!(out.contains(r##"src="#""##));
        assert!(!out.contains("href=\"javascript"));
        assert!(out.contains(r#"<span class="muted">-</span>"#));
        assert!(!out.contains("\"><img>"));
        assert!(out.contains(r#"data-api-action="/api/apps/a&quot;1""#));
        assert!(out.contains("data-edit-app=\"{&quot;"));
    }

    #[test]
    fn assistant_form_escapes_values_and_sets_checked() {
        let cfg = AssistantConfigRecord {
            id: "default".into(),
            enabled: true,
            web_search_enabled: false,
            provider: "x\" onfocus=\"alert(1)".into(),
            base_url: "https://a.b/?q=<x>".into(),
            model_discovery_path: None,
            encrypted_api_key: None,
            encrypted_headers: None,
            model_id: None,
            updated_at: 0,
        };
        let out = assistant_form(&cfg).into_string();
        assert!(out.contains(r#"value="x&quot; onfocus=&quot;alert(1)""#));
        assert!(out.contains("&lt;x&gt;"));
        assert!(out.contains(" checked"));
        assert!(out.contains(r#"value="openai/gpt-4o-mini""#));
    }
}
