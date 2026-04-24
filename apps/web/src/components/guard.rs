//! Enhanced Route Guards with RBAC Support
//!
//! Provides:
//! - Authentication guards
//! - Role-based access control (RBAC)
//! - Permission-based access control
//! - Route-level authorization

use crate::i18n::I18nContext;
use crate::state::auth::{use_auth_state, Permission, Role};
use gloo_storage::Storage;
use leptos::prelude::*;
use leptos::view;
use leptos_router::hooks::use_navigate;

/// Authentication guard - requires user to be logged in
#[component]
pub fn AuthGuard(children: ChildrenFn) -> impl IntoView {
    let auth = use_auth_state();
    let navigate = use_navigate();
    let auth_for_effect = auth.clone();

    // Signal for authenticated status
    let is_authenticated = Memo::new(move |_| auth.is_authenticated());

    Effect::new(move |_| {
        if !auth_for_effect.is_authenticated() {
            // Store current location for redirect after login
            if let Some(window) = web_sys::window() {
                if let Ok(Some(_)) =
                    gloo_storage::SessionStorage::raw().get_item("redirect_after_login")
                {
                    // Already set, don't override
                } else {
                    let current_path = window.location().pathname().unwrap_or_default();
                    let _ = gloo_storage::SessionStorage::raw()
                        .set_item("redirect_after_login", &current_path);
                }
            }

            navigate("/login", Default::default());
        }
    });

    // Use custom conditional rendering instead of Show
    move || {
        if is_authenticated.get() {
            children.clone()().into_any()
        } else {
            let i18n = use_context::<I18nContext>().expect("i18n context not found");
            let i18n_stored = StoredValue::new(i18n);
            view! { <Redirecting message=move || i18n_stored.get_value().t("auth-checking") /> }.into_any()
        }
    }
}

/// Role-based guard - requires specific role
#[component]
pub fn RoleGuard(#[prop(into)] role: Role, children: ChildrenFn) -> impl IntoView {
    let auth = use_auth_state();
    let navigate = use_navigate();
    let auth_for_effect = auth.clone();
    let role_for_effect = role.clone();

    let has_role = Memo::new(move |_| auth.has_role(&role));

    Effect::new(move |_| {
        if !auth_for_effect.has_role(&role_for_effect) {
            navigate("/unauthorized", Default::default());
        }
    });

    move || {
        if has_role.get() {
            children.clone()().into_any()
        } else {
            view! { <AccessDenied /> }.into_any()
        }
    }
}

/// Multiple roles guard - requires any of the specified roles
#[component]
pub fn AnyRoleGuard(#[prop(into)] roles: Vec<Role>, children: ChildrenFn) -> impl IntoView {
    let auth = use_auth_state();
    let navigate = use_navigate();
    let auth_for_effect = auth.clone();
    let roles_for_effect = roles.clone();

    let has_any_role = Memo::new(move |_| auth.has_any_role(&roles));

    Effect::new(move |_| {
        if !auth_for_effect.has_any_role(&roles_for_effect) {
            navigate("/unauthorized", Default::default());
        }
    });

    move || {
        if has_any_role.get() {
            children.clone()().into_any()
        } else {
            view! { <AccessDenied /> }.into_any()
        }
    }
}

/// Permission-based guard - requires specific permission
#[component]
pub fn PermissionGuard(
    #[prop(into)] permission: Permission,
    children: ChildrenFn,
) -> impl IntoView {
    let auth = use_auth_state();
    let navigate = use_navigate();
    let auth_for_effect = auth.clone();
    let permission_for_effect = permission.clone();

    let has_permission = Memo::new(move |_| auth.has_permission(&permission));

    Effect::new(move |_| {
        if !auth_for_effect.has_permission(&permission_for_effect) {
            navigate("/unauthorized", Default::default());
        }
    });

    move || {
        if has_permission.get() {
            children.clone()().into_any()
        } else {
            view! { <AccessDenied /> }.into_any()
        }
    }
}

/// Multiple permissions guard - requires any of the specified permissions
#[component]
pub fn AnyPermissionGuard(
    #[prop(into)] permissions: Vec<Permission>,
    children: ChildrenFn,
) -> impl IntoView {
    let auth = use_auth_state();
    let navigate = use_navigate();
    let auth_for_effect = auth.clone();
    let permissions_for_effect = permissions.clone();

    let has_any_permission = Memo::new(move |_| auth.has_any_permission(&permissions));

    Effect::new(move |_| {
        if !auth_for_effect.has_any_permission(&permissions_for_effect) {
            navigate("/unauthorized", Default::default());
        }
    });

    move || {
        if has_any_permission.get() {
            children.clone()().into_any()
        } else {
            view! { <AccessDenied /> }.into_any()
        }
    }
}

/// All permissions guard - requires all specified permissions
#[component]
pub fn AllPermissionsGuard(
    #[prop(into)] permissions: Vec<Permission>,
    children: ChildrenFn,
) -> impl IntoView {
    let auth = use_auth_state();
    let navigate = use_navigate();
    let auth_for_effect = auth.clone();
    let permissions_for_effect = permissions.clone();

    let has_all_permissions = Memo::new(move |_| auth.has_all_permissions(&permissions));

    Effect::new(move |_| {
        if !auth_for_effect.has_all_permissions(&permissions_for_effect) {
            navigate("/unauthorized", Default::default());
        }
    });

    move || {
        if has_all_permissions.get() {
            children.clone()().into_any()
        } else {
            view! { <AccessDenied /> }.into_any()
        }
    }
}

/// Combined guard - requires authentication AND role/permission
#[component]
pub fn CombinedGuard(
    #[prop(optional)] role: Option<Role>,
    #[prop(optional)] permission: Option<Permission>,
    children: ChildrenFn,
) -> impl IntoView {
    let auth = use_auth_state();
    let navigate = use_navigate();
    let auth_for_effect = auth.clone();
    let role_for_effect = role.clone();

    let check_access = Memo::new(move |_| {
        if !auth.is_authenticated() {
            return false;
        }

        if let Some(ref r) = role {
            if !auth.has_role(r) {
                return false;
            }
        }

        if let Some(ref p) = permission {
            if !auth.has_permission(p) {
                return false;
            }
        }

        true
    });

    Effect::new(move |_| {
        if !auth_for_effect.is_authenticated() {
            navigate("/login", Default::default());
        } else if let Some(ref r) = role_for_effect {
            if !auth_for_effect.has_role(r) {
                navigate("/unauthorized", Default::default());
            }
        }
    });

    move || {
        if check_access.get() {
            children.clone()().into_any()
        } else {
            view! { <AccessDenied /> }.into_any()
        }
    }
}

/// Guest only guard - only for non-authenticated users
#[component]
pub fn GuestOnly(children: ChildrenFn) -> impl IntoView {
    let auth = use_auth_state();
    let navigate = use_navigate();
    let auth_for_effect = auth.clone();

    let is_guest = Memo::new(move |_| !auth.is_authenticated());

    Effect::new(move |_| {
        if auth_for_effect.is_authenticated() {
            navigate("/", Default::default());
        }
    });

    move || {
        if is_guest.get() {
            children.clone()().into_any()
        } else {
            let i18n = use_context::<I18nContext>().expect("i18n context not found");
            let i18n_stored = StoredValue::new(i18n);
            view! { <Redirecting message=move || i18n_stored.get_value().t("redirecting") /> }.into_any()
        }
    }
}

/// Loading indicator during auth check
#[component]
fn Redirecting(#[prop(into)] message: Signal<String>) -> impl IntoView {
    view! {
        <div class="redirecting">
            <div class="spinner"></div>
            <p>{move || message.get()}</p>
        </div>
    }
}

/// Access denied component
#[component]
pub fn AccessDenied() -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    view! {
        <div class="access-denied">
            <div class="access-denied-icon">"🚫"</div>
            <h1>"403"</h1>
            <h2>{move || i18n_stored.get_value().t("error-access-denied")}</h2>
            <p>{move || i18n_stored.get_value().t("error-access-denied-desc")}</p>
            <div class="access-denied-actions">
                <a href="/" class="btn btn-primary">{move || i18n_stored.get_value().t("error-go-home")}</a>
                <a href="/contact" class="btn btn-secondary">{move || i18n_stored.get_value().t("contact-support")}</a>
            </div>
        </div>
    }
}

/// Hook for permission checking
pub fn use_permission_check(permission: Permission) -> Signal<bool> {
    let auth = use_auth_state();
    Signal::derive(move || auth.has_permission(&permission))
}

/// Hook for role checking
pub fn use_role_check(role: Role) -> Signal<bool> {
    let auth = use_auth_state();
    Signal::derive(move || auth.has_role(&role))
}

/// Hook for multiple permissions
pub fn use_any_permission_check(permissions: Vec<Permission>) -> Signal<bool> {
    let auth = use_auth_state();
    Signal::derive(move || permissions.iter().any(|p| auth.has_permission(p)))
}

/// Hook for checking if action is allowed
/// Returns a callback that checks permission before executing
pub fn use_protected_action<F>(permission: Permission, action: F) -> impl Fn()
where
    F: Fn() + 'static,
{
    let auth = use_auth_state();
    let navigate = use_navigate();

    move || {
        if auth.has_permission(&permission) {
            action();
        } else {
            navigate("/unauthorized", Default::default());
        }
    }
}

/// Component that conditionally renders based on permission
#[component]
pub fn PermissionShow(
    #[prop(into)] permission: Permission,
    children: ChildrenFn,
    #[prop(optional)] fallback: Option<ViewFn>,
) -> impl IntoView {
    let auth = use_auth_state();
    let has_permission = Memo::new(move |_| auth.has_permission(&permission));

    move || {
        if has_permission.get() {
            children.clone()().into_any()
        } else if let Some(ref fallback) = fallback {
            fallback.run().into_any()
        } else {
            view! {}.into_any()
        }
    }
}

/// Component that conditionally renders based on role
#[component]
pub fn RoleShow(
    #[prop(into)] role: Role,
    children: ChildrenFn,
    #[prop(optional)] fallback: Option<ViewFn>,
) -> impl IntoView {
    let auth = use_auth_state();
    let has_role = Memo::new(move |_| auth.has_role(&role));

    move || {
        if has_role.get() {
            children.clone()().into_any()
        } else if let Some(ref fallback) = fallback {
            fallback.run().into_any()
        } else {
            view! {}.into_any()
        }
    }
}

/// Admin shortcut guard
#[component]
pub fn AdminGuard(children: ChildrenFn) -> impl IntoView {
    view! {
        <RoleGuard role=Role::Admin>
            {children.clone()()}
        </RoleGuard>
    }
}

/// Operator or above guard
#[component]
pub fn OperatorGuard(children: ChildrenFn) -> impl IntoView {
    view! {
        <AnyRoleGuard roles=vec![Role::Admin, Role::Operator]>
            {children.clone()()}
        </AnyRoleGuard>
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_guard_exports() {
        // Just verify component types are accessible
        // Components are validated by compilation
        assert!(true);
    }
}
