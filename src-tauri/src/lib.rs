mod assist;
mod chat;
mod commands;
mod diff;
mod docs;
mod error;
mod git;
mod github;
mod links;
mod meta;
mod model;
mod notify;
mod proc;
mod stack;
mod term;
mod undo;

/// Which update mechanism this install supports. Windows/macOS bundles and the Linux
/// AppImage self-update via the Tauri updater ("updater"); a .deb/.rpm/pacman install does
/// not (no `APPIMAGE` env), so the UI only notifies and points at the package manager
/// ("manager"). Read by the frontend `AppUpdateBanner`.
#[tauri::command]
fn update_channel() -> &'static str {
    if cfg!(target_os = "linux") && std::env::var_os("APPIMAGE").is_none() {
        "manager"
    } else {
        "updater"
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Repair PATH before anything shells out, so `git`/`gh`/`claude` resolve even when
    // the app is launched from a GUI on macOS/Linux (no-op on Windows). See proc.rs.
    proc::fix_path_env();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(term::Terminals::default())
        .manage(chat::ChatSessions::default())
        .invoke_handler(tauri::generate_handler![
            commands::health,
            commands::set_ai_backend,
            commands::ollama_models,
            commands::get_repo_view,
            commands::clone_repo,
            commands::create_branch,
            commands::set_parent,
            commands::untrack_branch,
            commands::restack,
            commands::continue_restack,
            commands::abort_restack,
            commands::submit,
            commands::submit_plan,
            commands::sync,
            commands::merge_pr,
            commands::undo,
            commands::undo_peek,
            commands::list_stashes,
            commands::stash_count,
            commands::stash_push,
            commands::stash_apply,
            commands::stash_pop,
            commands::stash_drop,
            commands::checkout,
            commands::publish_branch,
            commands::branch_commits,
            commands::reword_commit,
            commands::drop_commit,
            commands::move_commit,
            commands::squash_commit,
            commands::split_diff,
            commands::split_commit,
            commands::cherry_pick,
            commands::stack_commits,
            commands::commit_detail,
            commands::repo_graph,
            commands::analyze_commit,
            commands::pr_detail,
            commands::submit_pr_review,
            commands::post_review_comments,
            commands::pr_checks,
            commands::review_pr,
            commands::review_commit,
            commands::generate_commit_message,
            commands::suggest_branch_name,
            commands::generate_pr_description,
            commands::suggest_conflict_resolution,
            commands::apply_conflict_resolution,
            commands::list_issues,
            commands::list_pull_requests,
            commands::issue_detail,
            commands::check_updates,
            commands::mark_updates_seen,
            commands::summarize_updates,
            commands::open_in_vscode,
            commands::list_markdown,
            commands::read_markdown,
            commands::create_markdown,
            update_channel,
            term::term_open_analyze,
            term::term_open_analyze_pr,
            term::term_open_merge_assist,
            term::term_open_merge_branches,
            term::term_write,
            term::term_resize,
            term::term_close,
            chat::chat_open_analyze,
            chat::chat_open_analyze_pr,
            chat::chat_open_repo,
            chat::chat_open_merge_pr,
            chat::chat_open_merge_branches,
            chat::chat_send,
            chat::chat_close
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
