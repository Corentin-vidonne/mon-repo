mod assist;
mod commands;
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(term::Terminals::default())
        .invoke_handler(tauri::generate_handler![
            commands::health,
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
            commands::checkout,
            commands::publish_branch,
            commands::branch_commits,
            commands::stack_commits,
            commands::commit_detail,
            commands::repo_graph,
            commands::analyze_commit,
            commands::pr_detail,
            commands::list_issues,
            commands::list_pull_requests,
            commands::issue_detail,
            commands::check_updates,
            commands::mark_updates_seen,
            commands::open_in_vscode,
            commands::list_markdown,
            commands::read_markdown,
            commands::create_markdown,
            term::term_open_analyze,
            term::term_open_analyze_pr,
            term::term_write,
            term::term_resize,
            term::term_close
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
