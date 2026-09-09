pub mod cli;
mod commands;
pub mod core;

use std::sync::Arc;

use core::cancel_token::CancelToken;
use core::skill_store::{default_db_path, migrate_legacy_db_if_needed, SkillStore};
use tauri::Manager;
use tauri_plugin_log::{Target, TargetKind};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            app.handle().plugin(
                tauri_plugin_log::Builder::default()
                    .level(log::LevelFilter::Info)
                    .targets([
                        Target::new(TargetKind::LogDir { file_name: None }),
                        #[cfg(desktop)]
                        Target::new(TargetKind::Stdout),
                    ])
                    .build(),
            )?;

            let db_path = default_db_path(app.handle()).map_err(tauri::Error::from)?;
            migrate_legacy_db_if_needed(&db_path).map_err(tauri::Error::from)?;
            let store = SkillStore::new(db_path);
            store.ensure_schema().map_err(tauri::Error::from)?;
            app.manage(store.clone());
            app.manage(Arc::new(CancelToken::new()));

            // Backfill description for skills that were installed before V2 schema.
            core::installer::backfill_skill_descriptions(&store);

            // ── Dev HTTP API (browser access) ────────────────────────
            // In debug builds, start a tiny HTTP server so the browser
            // frontend (localhost:5173) can fetch skills/config data
            // without Tauri IPC.
            #[cfg(debug_assertions)]
            {
                let api_store = store.clone();
                std::thread::spawn(move || {
                    let server = match tiny_http::Server::http("127.0.0.1:15723") {
                        Ok(s) => s,
                        Err(e) => {
                            log::warn!("HTTP API server failed to start: {e}");
                            return;
                        }
                    };
                    log::info!("HTTP API server listening on http://127.0.0.1:15723");

                    fn json_response(body: &str) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
                        tiny_http::Response::from_string(body)
                            .with_header(
                                tiny_http::Header::from_bytes(
                                    &b"Content-Type"[..],
                                    &b"application/json"[..],
                                )
                                .unwrap(),
                            )
                            .with_header(
                                tiny_http::Header::from_bytes(
                                    &b"Access-Control-Allow-Origin"[..],
                                    &b"*"[..],
                                )
                                .unwrap(),
                            )
                            .with_header(
                                tiny_http::Header::from_bytes(
                                    &b"Access-Control-Allow-Methods"[..],
                                    &b"GET, POST, OPTIONS"[..],
                                )
                                .unwrap(),
                            )
                            .with_header(
                                tiny_http::Header::from_bytes(
                                    &b"Access-Control-Allow-Headers"[..],
                                    &b"Content-Type, Authorization"[..],
                                )
                                .unwrap(),
                            )
                    }

                    for request in server.incoming_requests() {
                        let url = request.url().to_string();

                        // Handle CORS preflight
                        if request.method() == &tiny_http::Method::Options {
                            let _ = request.respond(json_response(""));
                            continue;
                        }

                        let response = match url.as_str() {
                            "/api/skills" => {
                                let skills = api_store.list_skills().unwrap_or_default();
                                let mut items = Vec::with_capacity(skills.len());
                                for rec in &skills {
                                    let targets: Vec<serde_json::Value> = api_store
                                        .list_skill_targets(&rec.id)
                                        .unwrap_or_default()
                                        .into_iter()
                                        .map(|t| {
                                            let target_path = std::path::Path::new(&t.target_path);
                                            let status = if target_path.exists() {
                                                "synced"
                                            } else {
                                                "missing"
                                            };
                                            serde_json::json!({
                                                "tool": t.tool,
                                                "scope": t.scope,
                                                "project_path": t.project_path,
                                                "mode": "symlink",
                                                "status": status,
                                                "target_path": t.target_path,
                                            })
                                        })
                                        .collect();
                                    let tags = api_store
                                        .get_skill_tags(&rec.id)
                                        .unwrap_or_default()
                                        .into_iter()
                                        .map(|t| serde_json::json!({ "id": t.id, "name": t.name }))
                                        .collect::<Vec<_>>();
                                    items.push(serde_json::json!({
                                        "id": rec.id,
                                        "name": rec.name,
                                        "description": rec.description,
                                        "source_type": rec.source_type,
                                        "source_ref": rec.source_ref,
                                        "central_path": rec.central_path,
                                        "created_at": rec.created_at,
                                        "updated_at": rec.updated_at,
                                        "last_sync_at": rec.last_sync_at,
                                        "status": rec.status,
                                        "tags": tags,
                                        "targets": targets,
                                    }));
                                }
                                let body = serde_json::to_string(&items).unwrap_or("[]".into());
                                json_response(&body)
                            }
                            "/api/tags" => {
                                let tags = api_store.list_tags_with_counts().unwrap_or_default();
                                let items: Vec<serde_json::Value> = tags
                                    .into_iter()
                                    .map(|t| serde_json::json!({
                                        "id": t.id,
                                        "name": t.name,
                                        "skill_count": t.skill_count,
                                        "updated_at": t.updated_at,
                                    }))
                                    .collect();
                                let body = serde_json::to_string(&items).unwrap_or("[]".into());
                                json_response(&body)
                            }
                            "/api/authors" => {
                                // Extract unique owners from skill_origins
                                let skills = api_store.list_skills().unwrap_or_default();
                                let mut owner_counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
                                for rec in &skills {
                                    if let Ok(Some(origin)) = api_store.get_skill_origin(&rec.id) {
                                        if let Some(owner) = &origin.owner {
                                            if !owner.is_empty() {
                                                *owner_counts.entry(owner.clone()).or_insert(0) += 1;
                                            }
                                        }
                                    }
                                }
                                let mut items: Vec<serde_json::Value> = owner_counts
                                    .into_iter()
                                    .map(|(owner, cnt)| serde_json::json!({ "owner": owner, "skill_count": cnt }))
                                    .collect();
                                items.sort_by(|a, b| b["skill_count"].as_i64().unwrap_or(0).cmp(&a["skill_count"].as_i64().unwrap_or(0)));
                                let body = serde_json::to_string(&items).unwrap_or("[]".into());
                                json_response(&body)
                            }
                            "/api/current-author" => {
                                let body = api_store.get_setting(core::app_config::CURRENT_AUTHOR_KEY)
                                    .ok()
                                    .flatten()
                                    .unwrap_or_else(|| r#"{"name":"","email":"","githubLogin":"","githubUrl":"","source":""}"#.into());
                                json_response(&body)
                            }
                            "/api/config" => {
                                let cfg = core::app_config::load_app_config(&api_store);
                                let body = match cfg {
                                    Ok(c) => serde_json::to_string(&c).unwrap_or("{}".into()),
                                    Err(_) => "{}".into(),
                                };
                                json_response(&body)
                            }
                            "/api/health" => {
                                json_response(r#"{"status":"ok"}"#)
                            }
                            _ => tiny_http::Response::from_string("Not Found")
                                .with_status_code(404),
                        };
                        let _ = request.respond(response);
                    }
                });
            }

            // Best-effort cleanup of our own old git temp directories.
            // Safety:
            // - Only deletes directories that match prefix `skilldo-git-*`
            // - And contain our marker file `.skilldo-git-temp`
            // - And are older than the max age.
            let handle = app.handle().clone();
            let store_for_cleanup = store.clone();
            tauri::async_runtime::spawn(async move {
                let removed = core::temp_cleanup::cleanup_old_git_temp_dirs(
                    &handle,
                    std::time::Duration::from_secs(24 * 60 * 60),
                )
                .unwrap_or(0);
                if removed > 0 {
                    log::info!("cleaned up {} old git temp dirs", removed);
                }

                let cleanup_days =
                    core::cache_cleanup::get_git_cache_cleanup_days(&store_for_cleanup);
                if cleanup_days > 0 {
                    let max_age =
                        std::time::Duration::from_secs(cleanup_days as u64 * 24 * 60 * 60);
                    let removed =
                        core::cache_cleanup::cleanup_git_cache_dirs(&handle, max_age).unwrap_or(0);
                    if removed > 0 {
                        log::info!("cleaned up {} git cache dirs", removed);
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_central_repo_path,
            commands::set_central_repo_path,
            commands::get_recent_projects,
            commands::save_recent_project,
            commands::get_tool_status,
            commands::get_git_cache_cleanup_days,
            commands::get_git_cache_ttl_secs,
            commands::set_git_cache_cleanup_days,
            commands::set_git_cache_ttl_secs,
            commands::clear_git_cache_now,
            commands::get_onboarding_plan,
            commands::install_local,
            commands::list_local_skills_cmd,
            commands::install_local_selection,
            commands::install_git,
            commands::list_git_skills_cmd,
            commands::install_git_selection,
            commands::install_package,
            commands::sync_skill_dir,
            commands::sync_skill_to_tool,
            commands::unsync_skill_from_tool,
            commands::update_managed_skill,
            commands::check_managed_skill_update_cmd,
            commands::check_all_managed_skill_updates_cmd,
            commands::publish_managed_skill,
            commands::repoify_skill,
            commands::search_github,
            commands::get_github_token,
            commands::set_github_token,
            commands::get_origin_rules,
            commands::set_origin_rules,
            commands::set_skill_origin_override,
            commands::reset_skill_origin_override,
            commands::import_existing_skill,
            commands::get_managed_skills,
            commands::get_tags,
            commands::create_tag,
            commands::rename_tag,
            commands::delete_tag,
            commands::get_skill_tags,
            commands::set_skill_tags,
            commands::get_untagged_skill_ids,
            commands::delete_managed_skill,
            commands::get_featured_skills,
            commands::search_skills_online,
            commands::get_explore_sources,
            commands::save_explore_sources,
            commands::get_explore_skills,
            commands::list_skill_files,
            commands::read_skill_file,
            commands::cancel_current_operation,
            commands::get_tool_skills_dir_overrides,
            commands::set_tool_skills_dir_override,
            commands::reset_tool_skills_dir_override,
            commands::get_custom_scan_dirs,
            commands::add_custom_scan_dir,
            commands::remove_custom_scan_dir,
            commands::browse_directory_show_hidden,
            commands::get_app_config,
            commands::save_app_config,
            commands::export_config,
            commands::import_config,
            commands::validate_github_token,
            commands::list_github_owners,
            commands::write_text_file,
            commands::read_text_file,
            commands::export_full_backup_json,
            commands::backup_to_file,
            commands::backup_webdav,
            commands::restore_from_file,
            commands::restore_from_webdav,
            commands::set_webdav_config,
            commands::get_profile_sync_status,
            commands::get_device_sync_status,
            commands::pull_device_state,
            commands::publish_device_state,
            commands::sync_profile,
            commands::export_profile_to_file,
            commands::import_profile_from_file,
            commands::resolve_profile_conflicts,
            commands::repair_sources,
            commands::repair_source,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                window.app_handle().exit(0);
            }
        })
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|_app, _event| {});
}
