use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::Html,
};
use homarr_auth::OptionalAuth;
use homarr_db::repos::{boards, users};
use maud::{html, Markup};

use crate::icons::{self, icon};
use crate::layout::{render_page, PageContext, ServerUiSettings};
use crate::WebState;

fn auth_shell(heading: &str, subtitle: &str, inner: Markup) -> Markup {
    html! {
        div.auth-shell {
            div.auth-floating-bar {
                a.icon-btn href="/" title="Back to Home Board" aria-label="Back to Home Board" { (icon(icons::HOME)) }
                button.icon-btn type="button" data-action="toggle-theme" title="Toggle theme" aria-label="Toggle theme" { (icon(icons::SUN_MOON)) }
            }
            div.auth-card {
                div.auth-brand {
                    img.auth-logo src="/logo/homarr.svg" alt="Homarr";
                    h1.auth-title { (heading) }
                    p.auth-subtitle { (subtitle) }
                }
                (inner)
            }
        }
    }
}

fn password_toggle() -> Markup {
    html! {
        button.password-toggle type="button" data-toggle-password aria-label="Toggle password visibility" { (icon(icons::EYE)) }
    }
}

pub async fn login_page(
    OptionalAuth(session): OptionalAuth,
    State(web): State<WebState>,
    headers: HeaderMap,
) -> Html<String> {
    let state = &web.app;
    let locale = web.i18n.negotiate(&headers);
    let visible_boards = boards::list_all(&state.db).await.unwrap_or_default();

    let inner = html! {
        @if state.config.demo_mode {
            div.auth-demo-alert {
                div {
                    strong { "Demo Mode Active" }
                    div.muted style="font-size:.82rem;margin-top:2px" {
                        "Username: " code { "demo" } " · Password: " code { "demo" }
                    }
                }
                button.btn.btn-sm type="button" data-action="fill-demo-login" { "Fill" }
            }
        }
        form #login-form .form-grid style="max-width:none" data-api-form="/api/auth/login" data-redirect="/" {
            div.form-row {
                label for="name" { "Username" }
                input #name name="name" placeholder="Enter your username" required autofocus;
            }
            div.form-row {
                label for="password" { "Password" }
                div.password-wrap {
                    input #password name="password" type="password" placeholder="••••••••" required;
                    (password_toggle())
                }
            }
            button.btn.btn-primary.btn-full type="submit" style="margin-top:4px" {
                (icon(icons::USER)) span { "Sign In" }
            }
        }
    };
    let body = auth_shell(
        "Welcome back",
        "Sign in to access your Homarr dashboard.",
        inner,
    );

    let ui = ServerUiSettings::load(&state.db).await;
    let mut ctx = PageContext {
        session: session.as_ref(),
        demo_mode: state.config.demo_mode,
        demo_read_only: state.config.demo_read_only,
        boards: &visible_boards,
        background_image_url: Some("/images/demo-dashboard-background.svg"),
        opacity: Some(88),
        hide_header: true,
        ..PageContext::minimal("Sign In", body, &locale, &web.i18n)
    };
    ui.apply(&mut ctx);
    Html(render_page(ctx))
}

pub async fn invite_page(
    OptionalAuth(session): OptionalAuth,
    State(web): State<WebState>,
    Path(token): Path<String>,
    headers: HeaderMap,
) -> Html<String> {
    let state = &web.app;
    let locale = web.i18n.negotiate(&headers);

    let inner = html! {
        form.form-grid style="max-width:none" data-api-form="/api/auth/register" data-redirect="/" {
            input type="hidden" name="inviteToken" value=(token);
            div.form-row {
                label for="name" { "Username" }
                input #name name="name" required minlength="2" placeholder="Choose a username";
            }
            div.form-row {
                label for="email" { "Email (optional)" }
                input #email name="email" type="email" placeholder="you@homelab.local";
            }
            div.form-row {
                label for="password" { "Password" }
                input #password name="password" type="password" required minlength="8" placeholder="At least 8 characters";
            }
            button.btn.btn-primary.btn-full type="submit" { "Create Account" }
        }
    };
    let body = auth_shell(
        "Accept Invitation",
        "Create your Homarr account using the invitation link.",
        inner,
    );

    let ui = ServerUiSettings::load(&state.db).await;
    let mut ctx = PageContext {
        session: session.as_ref(),
        demo_mode: state.config.demo_mode,
        demo_read_only: state.config.demo_read_only,
        background_image_url: Some("/images/demo-dashboard-background.svg"),
        opacity: Some(88),
        hide_header: true,
        ..PageContext::minimal("Register", body, &locale, &web.i18n)
    };
    ui.apply(&mut ctx);
    Html(render_page(ctx))
}

pub async fn onboarding_page(
    OptionalAuth(session): OptionalAuth,
    State(web): State<WebState>,
    headers: HeaderMap,
) -> Html<String> {
    let state = &web.app;
    let locale = web.i18n.negotiate(&headers);
    let user_count = users::count(&state.db).await.unwrap_or(0);
    let welcome = web
        .i18n
        .t(&locale, "init.studio.welcome.title", "Welcome home.");

    let inner = html! {
        div style="display:flex;justify-content:center;margin-bottom:14px" {
            span.demo-pill { "Homarr v" (env!("CARGO_PKG_VERSION")) }
        }
        @if user_count == 0 {
            div.init-step-box {
                h3 { "1. Administrator Credentials" }
                p.muted style="margin-top:0;font-size:.86rem" { "Set up your first administrator account and workspace defaults." }
                form.form-grid style="max-width:none" data-api-form="/api/auth/register" data-redirect="/" {
                    div.form-row { label { "Username" } input name="name" required minlength="2" placeholder="admin"; }
                    div.form-row { label { "Email (optional)" } input name="email" type="email" placeholder="admin@homarr.local"; }
                    div.form-row {
                        label { "Password" }
                        div.password-wrap {
                            input name="password" type="password" required minlength="8" placeholder="At least 8 characters";
                            (password_toggle())
                        }
                    }
                    div.init-divider {}
                    h3 style="margin-bottom:4px" { "2. Initial Workspace Defaults" }
                    div style="display:grid;grid-template-columns:1fr 1fr;gap:10px" {
                        div.form-row { label { "First Board Name" } input name="boardName" value="dashboard" pattern="[A-Za-z0-9_\\-]+"; }
                        div.form-row {
                            label { "Color Scheme" }
                            select name="colorScheme" {
                                option value="dark" selected { "Dark" }
                                option value="light" { "Light" }
                            }
                        }
                    }
                    label style="display:flex;align-items:center;gap:8px;font-weight:500" {
                        input type="checkbox" name="createStarterBoard" style="width:auto" checked;
                        " Create starter board with useful apps & widgets"
                    }
                    button.btn.btn-primary.btn-full type="submit" { "Create Administrator & Build Board" }
                }
            }
            p.muted style="font-size:.8rem;text-align:center;margin:0" {
                "Have an existing backup? Create your administrator account first, then import it under "
                strong { "Manage → Backup & Restore" } "."
            }
        } @else {
            div.init-step-box {
                h3 { "Workspace Ready" }
                p.muted style="margin-top:0;font-size:.86rem" { "Your administrator account is configured." }
                a.btn.btn-primary.btn-full href="/" { "Open Home Board" }
            }
            div.init-checklist {
                div.muted style="font-size:.78rem;text-transform:uppercase;letter-spacing:.05em;margin-bottom:8px" { "Quick Setup" }
                div style="display:flex;gap:8px;flex-wrap:wrap" {
                    a.btn.btn-sm href="/manage/boards" { "Boards" }
                    a.btn.btn-sm href="/manage/apps" { "Applications" }
                    a.btn.btn-sm href="/manage/integrations" { "Integrations" }
                    a.btn.btn-sm href="/manage/settings" { "Settings" }
                }
            }
        }
    };
    let body = auth_shell(
        &welcome,
        "Guided onboarding studio for your Homarr workspace.",
        inner,
    );

    let ui = ServerUiSettings::load(&state.db).await;
    let mut ctx = PageContext {
        session: session.as_ref(),
        demo_mode: state.config.demo_mode,
        demo_read_only: state.config.demo_read_only,
        background_image_url: Some("/images/demo-dashboard-background.svg"),
        opacity: Some(88),
        hide_header: true,
        ..PageContext::minimal("Onboarding", body, &locale, &web.i18n)
    };
    ui.apply(&mut ctx);
    Html(render_page(ctx))
}

pub async fn mcp_auth_page(
    OptionalAuth(session): OptionalAuth,
    State(web): State<WebState>,
    headers: HeaderMap,
) -> Html<String> {
    let state = &web.app;
    let locale = web.i18n.negotiate(&headers);
    let body = html! {
        div style="max-width:680px;margin:24px auto" {
            div.card {
                h1.page-title { "Homarr MCP Integration" }
                p.muted { "Connect AI agents and MCP clients to " code { "/api/mcp" } " via JSON-RPC 2.0." }
                ul {
                    li { strong { "Endpoint:" } " " code { "POST /api/mcp" } }
                    li {
                        strong { "Auth:" } " " code { "ApiKey: <id>.<secret>" }
                        " or " code { "Authorization: Bearer <token>" }
                    }
                    li { strong { "OAuth Protected Resource Metadata:" } " " code { "GET /.well-known/oauth-protected-resource" } }
                    li { strong { "Tools:" } " boards, apps, integrations, widgets, system info, cron jobs, Docker containers" }
                }
                a.btn.btn-primary href="/manage/tools/api-keys" { "Manage API Keys" }
            }
        }
    };

    let ui = ServerUiSettings::load(&state.db).await;
    let mut ctx = PageContext {
        session: session.as_ref(),
        demo_mode: state.config.demo_mode,
        demo_read_only: state.config.demo_read_only,
        ..PageContext::minimal("MCP Setup", body, &locale, &web.i18n)
    };
    ui.apply(&mut ctx);
    Html(render_page(ctx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_shell_escapes_text() {
        let out = auth_shell("<script>x</script>", "a\"b", html! {}).into_string();
        assert!(out.contains("&lt;script&gt;x&lt;/script&gt;"));
        assert!(!out.contains("<script>x"));
    }
}
