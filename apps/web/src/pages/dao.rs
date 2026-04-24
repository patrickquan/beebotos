use crate::api::{CreateProposalRequest, DaoSummary, ProposalInfo, ProposalStatus};
use crate::components::Modal;
use crate::i18n::I18nContext;
use crate::state::notification::NotificationType;
use crate::state::use_app_state;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::view;
use leptos_meta::*;
use leptos_router::components::A;

#[component]
pub fn DaoPage() -> impl IntoView {
    let app_state = use_app_state();
    let app_state_clone1 = app_state.clone();
    let app_state_clone2 = app_state.clone();

    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    // Fetch DAO summary - use LocalResource for CSR
    let dao_summary = LocalResource::new(move || {
        let service = app_state_clone1.dao_service();
        let loading = app_state_clone1.loading();
        async move {
            loading.dao.set(true);
            let result = service.get_summary().await;
            loading.dao.set(false);
            result
        }
    });

    // Fetch proposals
    let proposals = LocalResource::new(move || {
        let service = app_state_clone2.dao_service();
        async move { service.list_proposals().await }
    });

    // Create proposal modal state
    let create_open = RwSignal::new(false);
    let create_title = RwSignal::new(String::new());
    let create_desc = RwSignal::new(String::new());
    let create_type = RwSignal::new("general".to_string());
    let create_saving = RwSignal::new(false);
    let create_error = RwSignal::new(None::<String>);

    let on_create = move || {
        let req = CreateProposalRequest {
            title: create_title.get(),
            description: create_desc.get(),
            proposal_type: create_type.get(),
        };
        create_saving.set(true);
        create_error.set(None);
        let service = app_state.dao_service();
        spawn_local(async move {
            match service.create_proposal(req).await {
                Ok(_) => {
                    create_saving.set(false);
                    create_open.set(false);
                    create_title.set(String::new());
                    create_desc.set(String::new());
                    proposals.refetch();
                }
                Err(e) => {
                    create_saving.set(false);
                    create_error.set(Some(format!("{}: {}", i18n_stored.get_value().t("dao-create-failed"), e)));
                }
            }
        });
    };

    view! {
        <Title text={move || format!("{} - BeeBotOS", i18n_stored.get_value().t("dao-title"))} />
        <div class="page dao-page">
            <div class="page-header">
                <div>
                    <h1>{move || i18n_stored.get_value().t("dao-title")}</h1>
                    <p class="page-description">{move || i18n_stored.get_value().t("dao-subtitle")}</p>
                </div>
                <A href="/dao/treasury" attr:class="btn btn-secondary">
                    {move || i18n_stored.get_value().t("dao-view-treasury")}
                </A>
            </div>

            <Suspense fallback=|| view! { <DaoSummaryLoading/> }>
                {move || Suspend::new(async move {
                    match dao_summary.await {
                        Ok(data) => view! { <DaoSummaryView summary=data/> }.into_any(),
                        Err(_) => view! { <DaoSummaryPlaceholder/> }.into_any(),
                    }
                })}
            </Suspense>

            <section class="proposals-section">
                <div class="section-header">
                    <h2>{move || i18n_stored.get_value().t("dao-governance-proposals")}</h2>
                    <button class="btn btn-primary" on:click=move |_| create_open.set(true)>{move || format!("+ {}", i18n_stored.get_value().t("dao-create-proposal"))}</button>
                </div>

                // Create Proposal Modal
                {move || {
                    let on_create = on_create.clone();
                    if create_open.get() {
                    view! {
                        <Modal title=i18n_stored.get_value().t("modal-create-proposal-title") on_close=move || create_open.set(false)>
                            <div class="modal-body">
                                {move || create_error.get().map(|msg| view! {
                                    <div class="alert alert-error">{msg}</div>
                                })}
                                <div class="form-group">
                                    <label>{move || i18n_stored.get_value().t("label-title")}</label>
                                    <input
                                        type="text"
                                        prop:value=create_title
                                        on:input=move |e| create_title.set(event_target_value(&e))
                                        placeholder={move || i18n_stored.get_value().t("placeholder-proposal-title")}
                                    />
                                </div>
                                <div class="form-group">
                                    <label>{move || i18n_stored.get_value().t("label-description")}</label>
                                    <textarea
                                        prop:value=create_desc
                                        on:input=move |e| create_desc.set(event_target_value(&e))
                                        placeholder={move || i18n_stored.get_value().t("placeholder-proposal-desc")}
                                    />
                                </div>
                                <div class="form-group">
                                    <label>{move || i18n_stored.get_value().t("label-type")}</label>
                                    <select
                                        prop:value=create_type
                                        on:change=move |e| create_type.set(event_target_value(&e))
                                    >
                                        <option value="general">{move || i18n_stored.get_value().t("proposal-type-general")}</option>
                                        <option value="funding">{move || i18n_stored.get_value().t("proposal-type-funding")}</option>
                                        <option value="upgrade">{move || i18n_stored.get_value().t("proposal-type-upgrade")}</option>
                                        <option value="parameter">{move || i18n_stored.get_value().t("proposal-type-parameter")}</option>
                                    </select>
                                </div>
                            </div>
                            <div class="modal-footer">
                                <button class="btn btn-secondary" on:click=move |_| create_open.set(false)>{move || i18n_stored.get_value().t("action-cancel")}</button>
                                <button
                                    class="btn btn-primary"
                                    on:click={
                                        let on_create = on_create.clone();
                                        move |_| on_create()
                                    }
                                    disabled=create_saving
                                >
                                    {move || if create_saving.get() { i18n_stored.get_value().t("action-creating") } else { i18n_stored.get_value().t("dao-create-proposal") }}
                                </button>
                            </div>
                        </Modal>
                    }.into_any()
                } else {
                    ().into_any()
                }
                }}

                <Suspense fallback=|| view! { <ProposalsLoading/> }>
                    {move || Suspend::new(async move {
                        match proposals.await {
                            Ok(data) => {
                                if data.is_empty() {
                                    view! { <ProposalsEmpty/> }.into_any()
                                } else {
                                    view! { <ProposalsList proposals=data/> }.into_any()
                                }
                            }
                            Err(e) => view! {
                                <div class="error-message">
                                    {format!("{}: {}", i18n_stored.get_value().t("dao-load-proposals-failed"), e.to_string())}
                                </div>
                            }.into_any(),
                        }
                    })}
                </Suspense>
            </section>
        </div>
    }
}

#[component]
fn DaoSummaryView(summary: DaoSummary) -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    view! {
        <section class="dao-summary">
            <div class="stat-card">
                <div class="stat-value">{summary.member_count}</div>
                <div class="stat-label">{move || i18n_stored.get_value().t("dao-members")}</div>
            </div>
            <div class="stat-card">
                <div class="stat-value">{summary.active_proposals}</div>
                <div class="stat-label">{move || i18n_stored.get_value().t("dao-active-proposals")}</div>
            </div>
            <div class="stat-card">
                <div class="stat-value">{summary.user_voting_power}</div>
                <div class="stat-label">
                    {format!("{} ({})", i18n_stored.get_value().t("dao-your-voting-power"), summary.token_symbol)}
                </div>
            </div>
            <div class="stat-card">
                <div class="stat-value">{summary.token_balance}</div>
                <div class="stat-label">
                    {format!("{} ({})", i18n_stored.get_value().t("dao-your-balance"), summary.token_symbol)}
                </div>
            </div>
        </section>
    }
}

#[component]
fn DaoSummaryPlaceholder() -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    view! {
        <section class="dao-summary">
            <div class="stat-card skeleton">
                <div class="stat-value">"-"</div>
                <div class="stat-label">{move || i18n_stored.get_value().t("dao-members")}</div>
            </div>
            <div class="stat-card skeleton">
                <div class="stat-value">"-"</div>
                <div class="stat-label">{move || i18n_stored.get_value().t("dao-active-proposals")}</div>
            </div>
            <div class="stat-card skeleton">
                <div class="stat-value">"-"</div>
                <div class="stat-label">{move || i18n_stored.get_value().t("dao-your-voting-power")}</div>
            </div>
            <div class="stat-card skeleton">
                <div class="stat-value">"-"</div>
                <div class="stat-label">{move || i18n_stored.get_value().t("dao-your-balance")}</div>
            </div>
        </section>
    }
}

#[component]
fn DaoSummaryLoading() -> impl IntoView {
    view! { <DaoSummaryPlaceholder/> }
}

#[component]
fn ProposalsList(proposals: Vec<ProposalInfo>) -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);
    let (active, other): (Vec<_>, Vec<_>) = proposals
        .into_iter()
        .partition(|p| matches!(p.status, ProposalStatus::Active));

    view! {
        <div class="proposals-container">
            {move || if !active.is_empty() {
                view! {
                    <div class="proposals-group">
                        <h3>{move || i18n_stored.get_value().t("dao-active-proposals")}</h3>
                        <div class="proposals-list">
                            {active.clone().into_iter().map(|p| view! { <ProposalCard proposal=p/> }).collect::<Vec<_>>()}
                        </div>
                    </div>
                }.into_any()
            } else {
                view! { <></> }.into_any()
            }}

            {move || if !other.is_empty() {
                view! {
                    <div class="proposals-group">
                        <h3>{move || i18n_stored.get_value().t("dao-past-proposals")}</h3>
                        <div class="proposals-list">
                            {other.clone().into_iter().map(|p| view! { <ProposalCard proposal=p/> }).collect::<Vec<_>>()}
                        </div>
                    </div>
                }.into_any()
            } else {
                view! { <></> }.into_any()
            }}
        </div>
    }
}

#[component]
fn ProposalCard(#[prop(into)] proposal: ProposalInfo) -> impl IntoView {
    let _app_state = use_app_state();
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);
    let status_class = match proposal.status {
        ProposalStatus::Active => "status-active",
        ProposalStatus::Passed => "status-passed",
        ProposalStatus::Rejected => "status-rejected",
        ProposalStatus::Executed => "status-executed",
        ProposalStatus::Pending => "status-pending",
    };

    // Store proposal data in signals to avoid clone issues
    let proposal_id = proposal.id.clone();
    let proposal_id_for = proposal.id.clone();
    let proposal_id_against = proposal.id.clone();
    let votes_for = proposal.votes_for;
    let votes_against = proposal.votes_against;
    let is_active = proposal.status == ProposalStatus::Active;

    // Signal to track voting state (to prevent double voting)
    let user_voted = RwSignal::new(proposal.user_voted);
    let is_voting = RwSignal::new(false);

    view! {
        <div class="card proposal-card">
            <div class="proposal-header">
                <div class="proposal-title">
                    <h4>{proposal.title.clone()}</h4>
                    <span class=format!("status-badge {}", status_class)>
                        {format!("{:?}", proposal.status)}
                    </span>
                </div>
                <div class="proposal-meta">
                    <span>{format!("{} {}", i18n_stored.get_value().t("proposal-by"), proposal.proposer.clone())}</span>
                    <span>{format!("{}: {}", i18n_stored.get_value().t("dao-voting-ends"), proposal.ends_at.clone())}</span>
                </div>
            </div>

            <p class="proposal-description">{proposal.description.clone()}</p>

            <ProposalVotingSection
                _proposal_id={proposal_id}
                proposal_id_for={proposal_id_for}
                proposal_id_against={proposal_id_against}
                votes_for={votes_for}
                votes_against={votes_against}
                is_active={is_active}
                user_voted={user_voted}
                is_voting={is_voting}
            />
        </div>
    }
}

#[component]
fn ProposalVotingSection(
    _proposal_id: String,
    proposal_id_for: String,
    proposal_id_against: String,
    votes_for: u64,
    votes_against: u64,
    is_active: bool,
    user_voted: RwSignal<Option<bool>>,
    is_voting: RwSignal<bool>,
) -> impl IntoView {
    let _app_state = use_app_state();
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);
    let total_votes = votes_for + votes_against;
    let for_percent = if total_votes > 0 {
        (votes_for as f64 / total_votes as f64) * 100.0
    } else {
        0.0
    };

    view! {
        <div class={if is_active { "proposal-voting" } else { "proposal-results" }}>
            <div class="vote-bar">
                <div
                    class="vote-bar-for"
                    style={format!("width: {}%", for_percent)}
                ></div>
            </div>
            <div class="vote-stats">
                <span>{format!("{} {}", votes_for, i18n_stored.get_value().t("dao-votes-for"))}</span>
                <span>{format!("{} {}", votes_against, i18n_stored.get_value().t("dao-votes-against"))}</span>
            </div>

            {move || {
                let voted = user_voted.get();
                if is_active {
                    if voted.is_none() {
                        view! {
                            <div class="vote-actions">
                                <VoteButton
                                    proposal_id={proposal_id_for.clone()}
                                    vote_for={true}
                                    is_voting={is_voting}
                                    user_voted={user_voted}
                                />
                                <VoteButton
                                    proposal_id={proposal_id_against.clone()}
                                    vote_for={false}
                                    is_voting={is_voting}
                                    user_voted={user_voted}
                                />
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="voted-badge">
                                {if voted == Some(true) {
                                    format!("✓ {}", i18n_stored.get_value().t("voted-for")).into_any()
                                } else {
                                    format!("✓ {}", i18n_stored.get_value().t("voted-against")).into_any()
                                }}
                            </div>
                        }.into_any()
                    }
                } else {
                    view! { <></> }.into_any()
                }
            }}
        </div>
    }
}

#[component]
fn VoteButton(
    proposal_id: String,
    vote_for: bool,
    is_voting: RwSignal<bool>,
    user_voted: RwSignal<Option<bool>>,
) -> impl IntoView {
    let app_state = use_app_state();
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);
    let btn_class = if vote_for {
        "btn btn-success"
    } else {
        "btn btn-danger"
    };
    let label = if vote_for { i18n_stored.get_value().t("dao-vote-for") } else { i18n_stored.get_value().t("dao-vote-against") };

    view! {
        <button
            class={btn_class}
            disabled=is_voting
            on:click={
                let proposal_id = proposal_id.clone();
                move |_| {
                    let app_state = app_state.clone();
                    let proposal_id = proposal_id.clone();
                    is_voting.set(true);
                    spawn_local(async move {
                        let dao_service = app_state.dao_service();
                        match dao_service.vote(&proposal_id, vote_for, 1).await {
                            Ok(_) => {
                                user_voted.set(Some(vote_for));
                                app_state.notify(
                                    NotificationType::Success,
                                    &i18n_stored.get_value().t("notification-vote-submitted"),
                                    i18n_stored.get_value().t("notification-vote-submitted-desc")
                                );
                                dao_service.invalidate_proposals_cache();
                            }
                            Err(e) => {
                                app_state.notify(
                                    NotificationType::Error,
                                    &i18n_stored.get_value().t("notification-vote-failed"),
                                    format!("{}: {}", i18n_stored.get_value().t("notification-vote-failed-desc"), e)
                                );
                            }
                        }
                        is_voting.set(false);
                    });
                }
            }
        >
            {move || if is_voting.get() {
                i18n_stored.get_value().t("action-voting")
            } else {
                label.clone()
            }}
        </button>
    }
}

#[component]
fn ProposalsLoading() -> impl IntoView {
    view! {
        <div class="proposals-list">
            <div class="card proposal-card skeleton">
                <div class="skeleton-header"></div>
                <div class="skeleton-line"></div>
            </div>
            <div class="card proposal-card skeleton">
                <div class="skeleton-header"></div>
                <div class="skeleton-line"></div>
            </div>
        </div>
    }
}

#[component]
fn ProposalsEmpty() -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    view! {
        <div class="empty-state">
            <div class="empty-icon">"🏛️"</div>
            <h3>{move || i18n_stored.get_value().t("proposals-empty-title")}</h3>
            <p>{move || i18n_stored.get_value().t("proposals-empty-desc")}</p>
        </div>
    }
}
