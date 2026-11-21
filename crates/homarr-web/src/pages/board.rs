use std::collections::HashMap;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect},
};
use homarr_api::AppState;
use homarr_auth::{can_view_board, check_board_permission, AuthSession, OptionalAuth};
use homarr_db::{
    models::{BoardRecord, FullBoard, LayoutRecord, SectionRecord},
    repos::{boards, settings, users},
};
use homarr_definitions::{BoardPermission, WidgetKind};
use homarr_widgets::render_widget_html_with_origin;
use maud::{html, Markup, PreEscaped};
use serde_json::{json, Value};

use super::board_editor::{
    app_list, cell_style, compute_placements, edit_bar, editor_blob, editor_strings,
    layout_css, sorted_layouts, usable_integrations, widget_catalog, ItemPlacement,
};
use crate::icons::{self, icon};
use crate::layout::{
    build_spotlight_items, render_page, script_json, urlencoding, PageContext, ServerUiSettings,
};
use crate::WebState;

pub(crate) async fn list_visible_boards(
    state: &AppState,
    session: Option<&AuthSession>,
) -> Vec<BoardRecord> {
    let all = boards::list_all(&state.db).await.unwrap_or_default();
    let mut visible = Vec::new();
    for b in all {
        if can_view_board(&state.db, session, &b)
            .await
            .unwrap_or(false)
        {
            visible.push(b);
        }
    }
    visible
}

pub async fn home_page(
    OptionalAuth(session): OptionalAuth,
    State(web): State<WebState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let state = &web.app;
    if !state.config.demo_mode && users::count(&state.db).await.unwrap_or(0) == 0 {
        return Redirect::temporary("/init").into_response();
    }

    let visible = list_visible_boards(state, session.as_ref()).await;
    if visible.is_empty() {
        let locale = web.i18n.negotiate(&headers);
        let ui = ServerUiSettings::load(&state.db).await;
        let is_admin = session.as_ref().is_some_and(AuthSession::is_admin);
        let spotlight = build_spotlight_items(state, &visible, is_admin, false).await;
        let (text, href, cta) = if session.is_some() {
            (
                "No boards exist yet. Create your first board to start customizing your dashboard.",
                "/manage/boards",
                "Create Board",
            )
        } else {
            (
                "No public boards are visible yet. Sign in to access or create your boards.",
                "/auth/login",
                "Sign In",
            )
        };
        let body = html! {
            div.card.empty-board-card {
                h1.page-title { "Welcome to Homarr" }
                p.muted { (text) }
                div.btn-row-center {
                    a.btn.btn-primary href=(href) { (cta) }
                }
            }
        };
        let mut ctx = PageContext {
            session: session.as_ref(),
            demo_mode: state.config.demo_mode,
            demo_read_only: state.config.demo_read_only,
            boards: &visible,
            spotlight_items: &spotlight,
            ..PageContext::minimal("Home", body, &locale, &web.i18n)
        };
        ui.apply(&mut ctx);
        return Html(render_page(ctx)).into_response();
    }

    let default_board_id = settings::get_setting(&state.db, "board")
        .await
        .ok()
        .and_then(|v| {
            v.get("homeBoardId")
                .and_then(|x| x.as_str())
                .map(str::to_string)
        });

    let chosen = default_board_id
        .and_then(|id| visible.iter().find(|b| b.id == id))
        .or_else(|| visible.first())
        .expect("visible non-empty");

    let full = boards::get_full_board_by_id(&state.db, &chosen.id)
        .await
        .ok()
        .flatten();
    match full {
        Some(fb) => render_full_board(&web, session.as_ref(), &headers, &visible, fb)
            .await
            .into_response(),
        None => (StatusCode::NOT_FOUND, "Board not found").into_response(),
    }
}

pub async fn boards_index_page(
    OptionalAuth(session): OptionalAuth,
    State(web): State<WebState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let state = &web.app;
    let visible = list_visible_boards(state, session.as_ref()).await;
    if let Some(first) = visible.first() {
        return Redirect::temporary(&format!("/boards/{}", urlencoding(&first.name)))
            .into_response();
    }
    let locale = web.i18n.negotiate(&headers);
    let ui = ServerUiSettings::load(&state.db).await;
    let body = html! {
        div.card.empty-board-card {
            h1.page-title { "Boards" }
            p.muted { "No visible boards found." }
        }
    };
    let mut ctx = PageContext {
        session: session.as_ref(),
        demo_mode: state.config.demo_mode,
        demo_read_only: state.config.demo_read_only,
        boards: &visible,
        ..PageContext::minimal("Boards", body, &locale, &web.i18n)
    };
    ui.apply(&mut ctx);
    Html(render_page(ctx)).into_response()
}

pub async fn named_board_page(
    OptionalAuth(session): OptionalAuth,
    State(web): State<WebState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let state = &web.app;
    let visible = list_visible_boards(state, session.as_ref()).await;
    let Ok(Some(full)) = boards::get_full_board_by_name(&state.db, &name).await else {
        return (
            StatusCode::NOT_FOUND,
            Html("<h1>Board not found</h1>".to_string()),
        )
            .into_response();
    };
    if !can_view_board(&state.db, session.as_ref(), &full.board)
        .await
        .unwrap_or(false)
    {
        return Redirect::temporary("/auth/login").into_response();
    }
    render_full_board(&web, session.as_ref(), &headers, &visible, full)
        .await
        .into_response()
}

pub async fn board_settings_page(
    OptionalAuth(session): OptionalAuth,
    State(web): State<WebState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let state = &web.app;
    let Some(sess) = session else {
        return Redirect::temporary("/auth/login").into_response();
    };
    let visible = list_visible_boards(state, Some(&sess)).await;
    let Ok(Some(full)) = boards::get_full_board_by_name(&state.db, &name).await else {
        return (StatusCode::NOT_FOUND, "Board not found").into_response();
    };
    if check_board_permission(&state.db, Some(&sess), &full.board, BoardPermission::Modify)
        .await
        .is_err()
    {
        return (StatusCode::NOT_FOUND, "Board not found").into_response();
    }

    let locale = web.i18n.negotiate(&headers);
    let b = &full.board;
    let selected_radius = b.item_radius.as_deref().unwrap_or("lg");
    let select_opts = |current: &str, values: &[&str]| -> Markup {
        html! {
            @for v in values {
                option value=(v) selected[*v == current] { (v) }
            }
        }
    };
    let bid_url = urlencoding(&b.id);
    let opacity = b.opacity.clamp(10, 100);

    let body = html! {
        div.page-header {
            div {
                h1.page-title { "Board Settings: " (b.name) }
                p.muted style="margin:4px 0 0" { "Customize layout appearance, background, card glass opacity, and access. Columns and responsive layouts are edited in the board editor (Edit mode, Layouts)." }
            }
            div style="display:flex;gap:8px" {
                a.btn.btn-sm href={ "/boards/" (urlencoding(&b.name)) } { "Back to Board" }
                a.btn.btn-sm href={ "/api/boards/" (bid_url) "/export" } { "Export JSON" }
            }
        }
        div.card {
            form.form-grid data-api-form={ "/api/boards/" (bid_url) } data-method="PATCH" data-clear-empty {
                div.form-row { label { "Board Name" } input name="name" value=(b.name) required; }
                div.form-row { label { "Page Title" } input name="pageTitle" value=(b.page_title.as_deref().unwrap_or("")) placeholder="Homarr"; }
                div.form-row {
                    label { "Background Image URL" }
                    div style="display:flex;gap:8px" {
                        input #bg-url-input name="backgroundImageUrl" value=(b.background_image_url.as_deref().unwrap_or("")) placeholder="/images/demo-dashboard-background.svg";
                        button.btn.btn-sm type="button" style="white-space:nowrap" onclick="document.getElementById('bg-url-input').value='/images/demo-dashboard-background.svg'" { "Use Demo BG" }
                    }
                }
                div.form-row { label { "Meta Title" } input name="metaTitle" value=(b.meta_title.as_deref().unwrap_or("")); }
                div style="display:grid;grid-template-columns:1fr 1fr;gap:12px" {
                    div.form-row { label { "Logo Image URL" } input name="logoImageUrl" value=(b.logo_image_url.as_deref().unwrap_or("")) placeholder="/logo/homarr.svg"; }
                    div.form-row { label { "Favicon URL" } input name="faviconImageUrl" value=(b.favicon_image_url.as_deref().unwrap_or("")); }
                }
                div style="display:grid;grid-template-columns:1fr 1fr 1fr;gap:12px" {
                    div.form-row { label { "Background Attachment" } select name="backgroundImageAttachment" { (select_opts(&b.background_image_attachment, &["fixed", "scroll"])) } }
                    div.form-row { label { "Background Repeat" } select name="backgroundImageRepeat" { (select_opts(&b.background_image_repeat, &["no-repeat", "repeat", "repeat-x", "repeat-y"])) } }
                    div.form-row { label { "Background Size" } select name="backgroundImageSize" { (select_opts(&b.background_image_size, &["cover", "contain", "auto"])) } }
                }
                div.form-row {
                    label { "Card Opacity (" span #opacity-val { (opacity) "%" } ")" }
                    input type="range" name="opacity" min="10" max="100" value=(opacity) oninput="document.getElementById('opacity-val').textContent=this.value+'%'";
                }
                div.form-row {
                    label { "Card Border Radius" }
                    select name="itemRadius" {
                        @for (val, label) in [
                            ("xs", "Extra Small (4px)"),
                            ("sm", "Small (6px)"),
                            ("md", "Medium (8px)"),
                            ("lg", "Large (12px)"),
                            ("xl", "Extra Large (16px)"),
                        ] {
                            option value=(val) selected[selected_radius == val] { (label) }
                        }
                    }
                }
                div style="display:grid;grid-template-columns:1fr 1fr;gap:12px" {
                    div.form-row {
                        label { "Primary Accent Color" }
                        input type="color" name="primaryColor" value=(b.primary_color.as_deref().unwrap_or("#fa5252")) style="height:40px;padding:4px";
                    }
                    div.form-row {
                        label { "Secondary Accent Color" }
                        input type="color" name="secondaryColor" value=(b.secondary_color.as_deref().unwrap_or("#fd7e14")) style="height:40px;padding:4px";
                    }
                }
                div.form-row { label { "Custom CSS" } textarea name="customCss" rows="5" { (b.custom_css.as_deref().unwrap_or("")) } }
                div style="display:flex;gap:18px;flex-wrap:wrap" {
                    label style="display:flex;align-items:center;gap:8px" { input type="checkbox" name="isPublic" style="width:auto" checked[b.is_public]; " Public Board" }
                    label style="display:flex;align-items:center;gap:8px" { input type="checkbox" name="disableStatus" style="width:auto" checked[b.disable_status]; " Disable App Status Pings" }
                }
                div { button.btn.btn-primary type="submit" { "Save Settings" } }
            }
        }
    };

    let ui = ServerUiSettings::load(&state.db).await;
    let spotlight = build_spotlight_items(state, &visible, sess.is_admin(), false).await;
    let mut ctx = PageContext {
        session: Some(&sess),
        demo_mode: state.config.demo_mode,
        demo_read_only: state.config.demo_read_only,
        boards: &visible,
        active_board_name: Some(&b.name),
        spotlight_items: &spotlight,
        ..PageContext::minimal("Board Settings", body, &locale, &web.i18n)
    };
    ui.apply(&mut ctx);
    Html(render_page(ctx)).into_response()
}

fn advanced_str<'a>(adv: &'a Value, key: &str) -> Option<&'a str> {
    adv.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

fn is_hex_color(value: &str) -> bool {
    value.strip_prefix('#').is_some_and(|h| {
        matches!(h.len(), 3 | 4 | 6 | 8) && h.chars().all(|c| c.is_ascii_hexdigit())
    })
}

/// Class names allowed on a grid item (letters, digits, `-` and `_` only).
fn safe_css_classes(adv: &Value) -> Vec<String> {
    adv.get("customCssClasses")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .filter(|c| {
                    !c.is_empty()
                        && c.len() <= 64
                        && c.chars()
                            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
                })
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Per-layout CSS for container sections: their inner row count follows the active breakpoint,
/// and in edit mode their cell follows the layout picked in the editor (items get that from
/// `board-editor.js` through `--ex`/`--ey`/...; containers are not editor items).
fn container_layout_css(layouts: &[&LayoutRecord]) -> String {
    let mut css = String::new();
    for (i, layout) in layouts.iter().enumerate() {
        let rule = format!(".board-container{{--r:var(--r{i})}}");
        if layout.breakpoint <= 0 || i == 0 {
            css.push_str(&rule);
        } else {
            css.push_str(&format!(
                "@media (min-width:{}px){{{rule}}}",
                layout.breakpoint
            ));
        }
        if layout
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            css.push_str(&format!(
                r#"body[data-edit-layout="{id}"] .board-container{{--ex:var(--x{i});--ey:var(--y{i});--ew:var(--w{i});--eh:var(--h{i});--r:var(--r{i})}}"#,
                id = layout.id,
            ));
        }
    }
    css
}

enum GridNode<'a> {
    Item(&'a str),
    Container(&'a SectionRecord),
}

impl<'a> GridNode<'a> {
    fn id(&self) -> &'a str {
        match self {
            Self::Item(id) => id,
            Self::Container(sec) => &sec.id,
        }
    }
}

/// Renders section grids, recursing into the container sections placed inside them.
struct ContainerRenderer<'a> {
    children: HashMap<&'a str, Vec<GridNode<'a>>>,
    item_html: HashMap<&'a str, Markup>,
    placements: &'a HashMap<String, ItemPlacement>,
    collapsed: &'a HashMap<String, bool>,
    /// Set for signed-in viewers: collapse toggles are then stored per user on the server.
    collapse_board_id: Option<&'a str>,
    open_all_label: String,
    untitled_label: String,
}

impl<'a> ContainerRenderer<'a> {
    fn add_child(&mut self, parent: &'a str, node: GridNode<'a>) {
        self.children.entry(parent).or_default().push(node);
    }

    /// Orders every grid by first-layout position, so DOM order follows the visual order.
    fn sort(&mut self) {
        let placements = self.placements;
        for list in self.children.values_mut() {
            list.sort_by_key(|n| {
                let (y, x) = placements
                    .get(n.id())
                    .and_then(|p| p.cells.first())
                    .map(|c| (c.y, c.x))
                    .unwrap_or((0, 0));
                (y, x, n.id())
            });
        }
    }

    fn item_count(&self, section_id: &str) -> usize {
        self.children.get(section_id).map_or(0, |list| {
            list.iter()
                .filter(|n| matches!(n, GridNode::Item(_)))
                .count()
        })
    }

    fn grid(&self, section_id: &str) -> Markup {
        html! {
            @for node in self.children.get(section_id).into_iter().flatten() {
                @match node {
                    GridNode::Item(id) => { @if let Some(item) = self.item_html.get(id) { (item) } }
                    GridNode::Container(sec) => { (self.container(sec)) }
                }
            }
        }
    }

    /// A container section: a card placed in its parent grid with its own inner grid whose
    /// column count is the container's width (see `.container-grid` in homarr.css).
    fn container(&self, sec: &SectionRecord) -> Markup {
        let Some(pl) = self.placements.get(&sec.id) else {
            return html! {};
        };
        let opts = &sec.options;
        let flag =
            |key: &str, default: bool| opts.get(key).and_then(Value::as_bool).unwrap_or(default);
        let collapsible = flag("collapsible", false);
        let show_label = flag("showLabel", true);
        let show_open_all = flag("showOpenAll", false);
        let scrollable = flag("scrollable", false);
        let title = opts
            .get("title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|t| !t.is_empty());
        let label = title.unwrap_or(self.untitled_label.as_str());
        let collapsed = collapsible && self.collapsed.get(&sec.id).copied().unwrap_or(false);

        let kids = self.children.get(sec.id.as_str());
        let mut style = cell_style(&pl.cells);
        for (i, cell) in pl.cells.iter().enumerate() {
            let content_rows = kids
                .into_iter()
                .flatten()
                .filter_map(|n| self.placements.get(n.id())?.cells.get(i))
                .map(|c| c.y + c.h)
                .max()
                .unwrap_or(0);
            // Non-scrollable containers show all their content (rows shrink to fit the card).
            let rows = if scrollable {
                cell.h
            } else {
                cell.h.max(content_rows)
            };
            style.push_str(&format!("--r{i}:{rows};"));
        }
        if let Some(color) = advanced_str(opts, "borderColor").filter(|c| is_hex_color(c)) {
            style.push_str(&format!("border-color:{color};border-style:solid;"));
        }
        let mut classes = String::new();
        for c in safe_css_classes(opts) {
            classes.push(' ');
            classes.push_str(&c);
        }

        let sid = &sec.id;
        let expanded = if collapsed { "false" } else { "true" };
        let controls = format!("board-container-{sid}-content");
        let has_label = show_label && title.is_some();
        let board_id = self.collapse_board_id.filter(|_| collapsible);

        html! {
            div class={ "grid-item board-section board-container" (classes) } style=(style) data-section-id=(sid) data-board-id=[board_id] data-collapsible=(collapsible) data-collapsed=(collapsed) data-scrollable=(scrollable) {
                @if collapsible || has_label || show_open_all {
                    div.board-container-bar {
                        @if collapsible && show_label {
                            button.board-container-header type="button" data-container-toggle onclick="window.toggleCategory(this)" aria-expanded=(expanded) aria-controls=(controls) title=(label) {
                                span.board-container-title { (label) }
                                span.category-chevron { (icon(icons::CHEVRON_DOWN)) }
                            }
                        } @else if collapsible {
                            button.board-container-toggle type="button" data-container-toggle onclick="window.toggleCategory(this)" aria-expanded=(expanded) aria-controls=(controls) aria-label=(label) title=(label) {
                                span.category-chevron { (icon(icons::CHEVRON_DOWN)) }
                            }
                        } @else if has_label {
                            span.board-container-label title=(label) { (label) }
                        }
                        @if show_open_all {
                            @let name = self.open_all_label.replace("{name}", label);
                            button.icon-btn.icon-btn-sm.board-container-open-all type="button" onclick="window.openContainerApps(this)" title=(name) aria-label=(name) { (icon(icons::EXTERNAL_LINK)) }
                        }
                    }
                }
                div.board-container-body id=(controls) {
                    div.board-grid.container-grid data-layouts { (self.grid(&sec.id)) }
                }
            }
        }
    }
}

async fn render_full_board(
    web: &WebState,
    session: Option<&AuthSession>,
    headers: &HeaderMap,
    visible_boards: &[BoardRecord],
    full: FullBoard,
) -> Html<String> {
    let state = &web.app;
    let locale = web.i18n.negotiate(headers);
    let can_edit = !state.demo_read_only_active()
        && match session {
            Some(sess) => {
                check_board_permission(&state.db, Some(sess), &full.board, BoardPermission::Modify)
                    .await
                    .is_ok()
            }
            None => false,
        };
    let request_origin = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get("host"))
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| state.config.base_url.clone());

    let layouts = sorted_layouts(&full);
    let placements = compute_placements(&full, &layouts);
    let col_count = layouts
        .iter()
        .map(|l| l.column_count)
        .max()
        .unwrap_or(12)
        .clamp(1, 24);
    let mut layout_style = layout_css(&layouts);
    if layouts.is_empty() {
        layout_style = format!(".board-grid{{--board-cols:{col_count}}}");
    }

    layout_style.push_str(&container_layout_css(&layouts));

    // Container sections are rendered inside their parent's grid, not as stacked sections.
    let mut ordered_sections: Vec<_> = full
        .sections
        .iter()
        .filter(|s| s.kind != "container")
        .collect();
    ordered_sections.sort_by_key(|s| (s.y_offset.unwrap_or(0), s.x_offset.unwrap_or(0)));
    let collapsed_sections: HashMap<String, bool> = match session {
        Some(sess) => boards::get_section_collapse_states(&state.db, &sess.user_id)
            .await
            .unwrap_or_default(),
        None => HashMap::new(),
    };

    let editor_strings = editor_strings(&web.i18n, &locale);
    let tr = |key: &str| -> String {
        editor_strings
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or(key)
            .to_string()
    };

    // Resolve all placed widgets concurrently (cached in the resolver) instead of one by one.
    let placed_items: Vec<_> = full
        .items
        .iter()
        .filter_map(|item| placements.get(&item.id).map(|pl| (item, pl)))
        .collect();
    let resolved = state
        .widgets
        .resolve_many(
            placed_items
                .iter()
                .map(|(item, _)| {
                    (
                        WidgetKind::from_str_opt(&item.kind).unwrap_or(WidgetKind::Notebook),
                        item.options.clone(),
                        item.integration_ids.clone(),
                    )
                })
                .collect(),
        )
        .await;

    let mut item_html: HashMap<&str, Markup> = HashMap::new();
    for ((item, pl), result) in placed_items.into_iter().zip(resolved) {
        let kind = WidgetKind::from_str_opt(&item.kind).unwrap_or(WidgetKind::Notebook);
        let data = result.unwrap_or_else(|e| json!({ "error": e.to_string() }));

        let mut options = item.options.clone();
        if let (Some(title), Some(obj)) = (
            advanced_str(&item.advanced_options, "title"),
            options.as_object_mut(),
        ) {
            obj.insert("title".into(), Value::String(title.to_string()));
        }
        let rendered_card = render_widget_html_with_origin(
            kind,
            &item.id,
            &options,
            &data,
            request_origin.as_deref(),
        );

        let mut extra_classes = String::new();
        for c in safe_css_classes(&item.advanced_options) {
            extra_classes.push(' ');
            extra_classes.push_str(&c);
        }
        let mut extra_style = cell_style(&pl.cells);
        if let Some(color) =
            advanced_str(&item.advanced_options, "borderColor").filter(|c| is_hex_color(c))
        {
            extra_style.push_str(&format!("border-color:{color};border-style:solid;"));
        }

        let (edit_t, dup_t, rm_t) = (tr("editItem"), tr("duplicateItem"), tr("removeItem"));
        item_html.insert(
            &item.id,
            html! {
                div class={ "grid-item" (extra_classes) } style=(extra_style) data-item-id=(item.id) data-kind=(kind.as_str()) {
                    @if can_edit {
                        div.item-edit-overlay data-editor-drag aria-hidden="true" {}
                        div.item-edit-controls {
                            button.icon-btn.icon-btn-sm type="button" title=(edit_t) aria-label=(edit_t) data-editor-action="edit-item" data-item-id=(item.id) { (icon(icons::PENCIL)) }
                            button.icon-btn.icon-btn-sm type="button" title=(dup_t) aria-label=(dup_t) data-editor-action="duplicate-item" data-item-id=(item.id) { (icon(icons::COPY)) }
                            button.icon-btn.icon-btn-sm.icon-btn-danger type="button" title=(rm_t) aria-label=(rm_t) data-editor-action="remove-item" data-item-id=(item.id) { (icon(icons::TRASH)) }
                        }
                        div.item-resize-handle data-editor-resize title="Resize" {}
                    }
                    // safe: rendered by homarr-widgets (escapes its own data)
                    // shortcut: String wrapped as raw HTML, drop PreEscaped once render.rs returns Markup
                    (PreEscaped(rendered_card))
                }
            },
        );
    }

    let mut grids = ContainerRenderer {
        children: HashMap::new(),
        item_html,
        placements: &placements,
        collapsed: &collapsed_sections,
        collapse_board_id: session.map(|_| full.board.id.as_str()),
        open_all_label: web.i18n.t(
            &locale,
            "section.action.openAllInNewTabsFor",
            "Open all apps in {name} in tabs",
        ),
        untitled_label: web
            .i18n
            .t(&locale, "section.container.untitled", "Untitled container"),
    };
    for item in &full.items {
        if let Some(pl) = placements.get(&item.id) {
            grids.add_child(&pl.section_id, GridNode::Item(&item.id));
        }
    }
    for sec in full.sections.iter().filter(|s| s.kind == "container") {
        if let Some(pl) = placements.get(&sec.id) {
            grids.add_child(&pl.section_id, GridNode::Container(sec));
        }
    }
    grids.sort();

    let (up_t, down_t, edit_t, rm_t) = (
        tr("moveUp"),
        tr("moveDown"),
        tr("editSection"),
        tr("removeSection"),
    );
    let sections_html = html! {
        @for sec in ordered_sections {
            @let sec_label = sec
                .name
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(if sec.kind == "category" { "Category" } else { "Main Canvas" });
            @let sid = &sec.id;
            @let section_edit_bar = html! {
                @if can_edit {
                    div.section-edit-bar {
                        span.section-edit-name { (sec_label) }
                        span.section-edit-actions {
                            button.icon-btn.icon-btn-sm type="button" title=(up_t) aria-label=(up_t) data-editor-action="section-up" data-section-id=(sid) { (icon(icons::ARROW_UP)) }
                            button.icon-btn.icon-btn-sm type="button" title=(down_t) aria-label=(down_t) data-editor-action="section-down" data-section-id=(sid) { (icon(icons::ARROW_DOWN)) }
                            button.icon-btn.icon-btn-sm type="button" title=(edit_t) aria-label=(edit_t) data-editor-action="section-edit" data-section-id=(sid) { (icon(icons::PENCIL)) }
                            button.icon-btn.icon-btn-sm.icon-btn-danger type="button" title=(rm_t) aria-label=(rm_t) data-editor-action="section-remove" data-section-id=(sid) { (icon(icons::TRASH)) }
                        }
                    }
                }
            };
            @if sec.kind == "category" {
                section.board-section.category-section data-section-id=(sid) data-collapsed="false" {
                    (section_edit_bar)
                    button.category-header type="button" onclick="window.toggleCategory(this)" aria-expanded="true" {
                        span.category-title-wrap {
                            span.category-chevron { (icon(icons::CHEVRON_DOWN)) }
                            span.category-title { (sec_label) }
                            span.category-count { (grids.item_count(sid)) }
                        }
                    }
                    div.category-body {
                        div.board-grid data-layouts { (grids.grid(sid)) }
                    }
                }
            } @else {
                section.board-section.board-section-empty data-section-id=(sid) {
                    (section_edit_bar)
                    @if let Some(name) = sec.name.as_deref().map(str::trim).filter(|n| !n.is_empty() && *n != "Main") {
                        div.section-header { h2.section-title { (name) } }
                    }
                    div.board-grid data-layouts { (grids.grid(sid)) }
                }
            }
        }
    };

    let (edit_mode_bar, editor_script) = if can_edit {
        let widgets = widget_catalog(&web.i18n, &locale);
        let app_list = app_list(&state.db).await;
        let integrations = match session {
            Some(sess) => usable_integrations(&state.db, sess).await,
            None => Vec::new(),
        };
        let blob = editor_blob(
            &full,
            &layouts,
            &placements,
            widgets,
            app_list,
            integrations,
            editor_strings.clone(),
        );
        let editor_js = format!("window.__HOMARR_EDITOR__ = {};", script_json(&blob));
        (
            edit_bar(&full.board.name, &editor_strings),
            html! {
                div #editor-modal-root {}
                // safe: script_json escapes `<`, `>` and `&`, so the data cannot close the tag
                script { (PreEscaped(editor_js)) }
            },
        )
    } else {
        (html! {}, html! {})
    };

    let page_title = full
        .board
        .page_title
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(&full.board.name);

    let body = html! {
        // safe: layout CSS is built from numbers and ids filtered to [A-Za-z0-9_-]
        style { (PreEscaped(layout_style)) }
        (edit_mode_bar)
        div.board-sections-stack { (sections_html) }
        (editor_script)
    };

    let ui = ServerUiSettings::load(&state.db).await;
    let is_admin = session.is_some_and(AuthSession::is_admin);
    let spotlight = build_spotlight_items(state, visible_boards, is_admin, can_edit).await;
    let mut ctx = PageContext {
        title: page_title,
        body,
        session,
        demo_mode: state.config.demo_mode,
        demo_read_only: state.config.demo_read_only,
        boards: visible_boards,
        active_board_name: Some(&full.board.name),
        locale: &locale,
        i18n: &web.i18n,
        custom_css: full.board.custom_css.as_deref(),
        primary_color: full.board.primary_color.as_deref(),
        secondary_color: full.board.secondary_color.as_deref(),
        background_image_url: full.board.background_image_url.as_deref(),
        background_image_attachment: Some(&full.board.background_image_attachment),
        background_image_repeat: Some(&full.board.background_image_repeat),
        background_image_size: Some(&full.board.background_image_size),
        opacity: Some(full.board.opacity),
        item_radius: full.board.item_radius.as_deref(),
        logo_image_url: full
            .board
            .logo_image_url
            .as_deref()
            .filter(|s| !s.trim().is_empty()),
        favicon_image_url: full.board.favicon_image_url.as_deref(),
        spotlight_items: &spotlight,
        can_edit_board: can_edit,
        is_board_page: true,
        hide_header: false,
        app_name: None,
        default_color_scheme: None,
        robots_meta: None,
    };
    ui.apply(&mut ctx);
    // Upstream shows the board's page title (falling back to the app name) next to the logo.
    if let Some(title) = full
        .board
        .page_title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        ctx.app_name = Some(title);
    }
    Html(render_page(ctx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pages::board_editor::Cell;

    #[test]
    fn container_escapes_title_and_id() {
        let sec = SectionRecord {
            id: "s\"1".into(),
            board_id: "b".into(),
            kind: "container".into(),
            x_offset: None,
            y_offset: None,
            name: None,
            options: json!({ "title": "<img src=x onerror=alert(1)>\"", "collapsible": true, "showOpenAll": true }),
            layouts: Vec::new(),
        };
        let cell = Cell {
            x: 0,
            y: 0,
            w: 2,
            h: 2,
            real: true,
        };
        let placements = HashMap::from([(
            sec.id.clone(),
            ItemPlacement {
                section_id: "root".into(),
                cells: vec![cell],
            },
        )]);
        let collapsed = HashMap::new();
        let r = ContainerRenderer {
            children: HashMap::new(),
            item_html: HashMap::new(),
            placements: &placements,
            collapsed: &collapsed,
            collapse_board_id: Some("b\"<"),
            open_all_label: "Open {name}".into(),
            untitled_label: "Untitled".into(),
        };
        let html = r.container(&sec).into_string();
        assert!(!html.contains("<img"), "{html}");
        assert!(html.contains(r#"data-section-id="s&quot;1""#), "{html}");
        assert!(html.contains(r#"data-board-id="b&quot;&lt;""#), "{html}");
        assert!(
            html.contains(r#"title="&lt;img src=x onerror=alert(1)&gt;&quot;""#),
            "{html}"
        );
        assert!(html.contains(r#"data-collapsible="true""#), "{html}");
    }
}
