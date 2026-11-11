use super::*;
use eframe::egui::{self, epaint::ClippedShape, Rect, Vec2};

#[test]
fn startup_keeps_remote_model_cache_without_requesting_models_when_keys_are_missing() {
    let context = egui::Context::default();
    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
    app.llm_config.api_key = "  ".into();
    app.repository_llm_configs
        .insert("/repo".into(), app.llm_config.clone());
    for scope in [None, Some("/repo")] {
        app.llm_models.entry(&app.llm_config, scope).options = vec!["cached-model".into()];
    }
    app.refresh_llm_models(&context);
    for scope in [None, Some("/repo")] {
        let models = app.llm_models.entry(&app.llm_config, scope);
        assert_eq!(models.options, ["cached-model"]);
        assert!(!models.attempted);
        assert!(!models.loading());
        assert!(models.error.is_none());
    }
}

#[test]
fn startup_refreshes_cached_models_without_loading_credentials_or_changing_selection() {
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
        time::{Duration, Instant},
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        for request in 0..4 {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "Startup did not request models");
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("Cannot accept test request: {error}"),
                }
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(&mut socket);
            loop {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
                assert!(!line.to_ascii_lowercase().starts_with("authorization:"));
            }
            let body = format!(r#"{{"data":[{{"id":"model-{}"}}]}}"#, request / 2);
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
    });
    let context = egui::Context::default();
    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
    app.llm_config = crate::llm::Config {
        endpoint: format!("http://{address}/v1"),
        model: "my-custom-model".into(),
        store_api_key: true,
        ..Default::default()
    };
    app.repository_llm_configs
        .insert("/repo".into(), app.llm_config.clone());
    for launch in 0..2 {
        if launch == 1 {
            let saved = serde_json::to_string(&app.llm_models).unwrap();
            app.llm_models = serde_json::from_str(&saved).unwrap();
            assert_eq!(
                app.llm_models.entry(&app.llm_config, None).options,
                ["model-0"]
            );
        }
        app.refresh_llm_models(&context);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            app.llm_models.update();
            let loading = [None, Some("/repo")]
                .into_iter()
                .any(|scope| app.llm_models.entry(&app.llm_config, scope).loading());
            if !loading {
                break;
            }
            assert!(Instant::now() < deadline, "Model refresh did not finish");
            std::thread::sleep(Duration::from_millis(5));
        }
        for scope in [None, Some("/repo")] {
            let models = app.llm_models.entry(&app.llm_config, scope);
            assert!(models.error.is_none(), "{:?}", models.error);
            assert_eq!(models.options, [format!("model-{launch}")]);
        }
        assert_eq!(app.llm_config.model, "my-custom-model");
        assert_eq!(app.repository_llm_configs["/repo"].model, "my-custom-model");
    }
    server.join().unwrap();
}

fn deliver(app: &mut SannyGitApp, result: WorkerResult) {
    let (sender, receiver) = mpsc::channel();
    app.worker = Some(receiver);
    sender.send(result).ok().unwrap();
    app.poll_worker();
}

#[test]
fn repository_error_retains_view_and_refresh_preserves_selection() {
    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
    app.repositories = crate::mock_data::repositories();
    app.changes = crate::mock_data::changes();
    app.selected_repository = 0;
    app.selected_change = 2;
    let selected = app.changes[2].path.clone();
    deliver(
        &mut app,
        WorkerResult::Repository(Err("Cannot read repository".into())),
    );
    assert_eq!(app.changes[app.selected_change].path, selected);
    assert!(app.error.is_some());
    let mut changes = crate::mock_data::changes();
    changes.reverse();
    let snapshot = git::Snapshot {
        repository: Repository {
            name: app.repositories[0].name.clone(),
            path: app.repositories[0].path.clone(),
        },
        branch: "real-branch".into(),
        changes,
    };
    deliver(&mut app, WorkerResult::Repository(Ok(snapshot)));
    assert_eq!(app.changes[app.selected_change].path, selected);
    assert_eq!(app.current_branch, "real-branch");
    assert!(app.commits.is_empty());
}

#[test]
fn changing_selection_hides_previous_diff_and_discards_late_result() {
    let mut app = SannyGitApp::with_theme(ThemeChoice::Dark);
    app.changes = crate::mock_data::changes();
    app.selected_change = 0;
    app.loaded_diff = Some(0);
    app.diff_preview = Some(crate::models::DiffPreview::new("+old"));
    assert!(app.selected_diff().is_some());
    app.selected_change = 1;
    assert!(app.selected_diff().is_none());
    deliver(
        &mut app,
        WorkerResult::Diff {
            index: 0,
            result: Err("stale error".into()),
        },
    );
    assert!(app.error.is_none());
    assert!(app.selected_diff().is_none());
}

#[test]
fn cancelling_generation_preserves_draft_and_discards_late_results() {
    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
    app.commit_message = "Existing draft".into();
    let cancel = Arc::new(AtomicBool::new(false));
    app.generation_cancel = Some(cancel.clone());
    app.generating_commit_message = true;
    let (sender, receiver) = mpsc::channel();
    app.worker = Some(receiver);
    sender
        .send(commit_message_result(Ok("Already queued response".into())))
        .unwrap();
    app.cancel_commit_message_generation();
    assert!(cancel.load(Ordering::Relaxed));
    assert!(!app.busy());
    assert!(!app.generating_commit_message);
    assert!(app.generation_cancel.is_none());
    assert_eq!(app.commit_message, "Existing draft");
    assert!(app.error.is_none());
    assert_eq!(app.status, "Commit message generation cancelled");
    assert!(sender
        .send(commit_message_result(Ok("Late response".into())))
        .is_err());

    // A new generation can finish normally; the old sender is isolated.
    app.generating_commit_message = true;
    deliver(&mut app, commit_message_result(Ok("New response".into())));
    assert_eq!(app.commit_message, "New response");
}

#[test]
fn cancelling_generation_does_not_cancel_other_workers() {
    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
    let (_sender, receiver) = mpsc::channel();
    app.worker = Some(receiver);
    app.status = "Loading repository".into();
    app.cancel_commit_message_generation();
    assert!(app.busy());
    assert_eq!(app.status, "Loading repository");
}

#[test]
fn generated_commit_message_updates_draft_and_errors_preserve_it() {
    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
    app.commit_message = "Existing draft".into();
    app.generating_commit_message = true;
    deliver(
        &mut app,
        commit_message_result(Ok("Generated message".into())),
    );
    assert_eq!(app.commit_message, "Generated message");
    assert_eq!(app.status, "Commit message generated");
    assert!(!app.generating_commit_message);

    app.generating_commit_message = true;
    deliver(
        &mut app,
        commit_message_result(Err("Provider unavailable".into())),
    );
    assert_eq!(app.commit_message, "Generated message");
    assert_eq!(app.error.as_deref(), Some("Provider unavailable"));
    assert!(!app.generating_commit_message);
}

#[test]
fn staged_mode_gates_commit_preview_and_generation_but_allows_message_only_amend() {
    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
    assert_eq!(app.commit_contents, git::CommitContents::All);
    app.repositories = crate::mock_data::repositories();
    app.selected_repository = 0;
    app.state_current = true;
    app.commit_message = "Draft".into();
    app.changes = crate::mock_data::changes();
    for change in &mut app.changes {
        change.staged = false;
        change.kind = crate::models::ChangeKind::Modified;
    }
    app.commit_contents = git::CommitContents::Staged;
    assert!(app.commit_blocker().unwrap().contains("no staged"));
    assert!(app
        .generate_commit_message_blocker()
        .unwrap()
        .contains("no staged"));
    app.changes[0].staged = true;
    assert!(app.commit_blocker().is_none());
    assert!(app.generate_commit_message_blocker().is_none());
    app.changes.clear();
    app.amend = true;
    app.amend_head = Some("a".repeat(40));
    assert!(app.commit_blocker().is_none());
    assert!(app.commit_contents_preview_blocker().is_none());
    assert!(app.generate_commit_message_blocker().is_some());
    assert_eq!(app.commit_message, "Draft");
}

#[test]
fn generation_requires_files_in_the_selected_mode_even_when_amending() {
    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
    app.repositories = crate::mock_data::repositories();
    app.selected_repository = 0;
    app.state_current = true;
    app.commit_message = "Draft".into();
    for amend in [false, true] {
        app.amend = amend;
        app.amend_head = amend.then(|| "a".repeat(40));
        for mode in [git::CommitContents::All, git::CommitContents::Staged] {
            app.commit_contents = mode;
            app.changes.clear();
            assert!(app.generate_commit_message_blocker().is_some());
            app.generate_commit_message();
            assert!(!app.busy());
            assert!(!app.generating_commit_message);
            assert_eq!(app.commit_message, "Draft");

            app.changes = crate::mock_data::changes();
            for change in &mut app.changes {
                change.staged = false;
            }
            assert_eq!(
                app.generate_commit_message_blocker().is_none(),
                mode == git::CommitContents::All
            );
            app.changes[0].staged = true;
            assert!(app.generate_commit_message_blocker().is_none());
        }
    }
}

#[test]
fn commit_and_generation_accept_unstaged_and_untracked_files() {
    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
    assert!(app.commit_blocker().is_some());
    app.repositories = crate::mock_data::repositories();
    app.selected_repository = 0;
    app.changes = crate::mock_data::changes();
    app.commit_message = "Message".into();
    assert!(app.commit_blocker().unwrap().contains("Refresh"));
    app.state_current = true;
    for change in &mut app.changes {
        change.staged = false;
    }
    assert!(app.commit_blocker().is_none());
    assert!(app.generate_commit_message_blocker().is_none());
    for change in &mut app.changes {
        change.kind = crate::models::ChangeKind::Untracked;
    }
    assert!(app.commit_blocker().is_none());
    assert!(app.generate_commit_message_blocker().is_none());
    let changes = std::mem::take(&mut app.changes);
    assert!(app.commit_blocker().is_some());
    assert!(app.generate_commit_message_blocker().is_some());
    app.changes = changes;
    app.changes[0].kind = crate::models::ChangeKind::Modified;
    app.commit_message = " \n\t".into();
    assert!(app.commit_blocker().unwrap().contains("message"));
    app.create_commit();
    assert!(!app.busy());
    app.commit_message = "Valid message".into();
    assert!(app.commit_blocker().is_none());
    app.changes[0].staged = true;
    assert!(app.staging_allowed(0, false));
    app.changes[0].kind = crate::models::ChangeKind::Conflicted;
    assert!(app.commit_blocker().unwrap().contains("conflicts"));
    assert!(!app.staging_allowed(0, true));
    assert!(!app.staging_allowed(0, false));
}

#[test]
fn amend_loads_head_message_restores_draft_and_allows_rewording() {
    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
    assert!(app.amend_blocker().is_some());
    app.repositories = crate::mock_data::repositories();
    app.selected_repository = 0;
    app.state_current = true;
    app.refs_current = true;
    assert!(app.amend_blocker().unwrap().contains("no commit"));
    app.branch_head_exists = true;
    assert!(app.amend_blocker().is_none());
    app.commit_message = "Draft".into();
    deliver(
        &mut app,
        WorkerResult::AmendMessage(Err("Cannot read HEAD".into())),
    );
    assert_eq!(app.commit_message, "Draft");
    assert!(!app.amend);
    deliver(
        &mut app,
        WorkerResult::AmendMessage(Ok(("a".repeat(40), "Last message\n\nBody".into()))),
    );
    assert!(app.amend);
    assert_eq!(app.commit_message, "Last message\n\nBody");
    assert!(app.changes.is_empty());
    assert!(app.commit_blocker().is_none());
    app.commit_message = "Edited message".into();
    app.set_amend(false);
    assert_eq!(app.commit_message, "Draft");
    assert!(!app.amend);
    assert!(app.amend_head.is_none());
    assert!(app.commit_blocker().unwrap().contains("no changes"));
}

#[test]
fn skipped_push_and_sync_preserve_errors_for_modal_reporting() {
    for operation in [
        crate::remotes::Operation::Push,
        crate::remotes::Operation::Sync,
    ] {
        for skip in [false, true] {
            for failed in [false, true] {
                for refresh_failed in [false, true] {
                    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
                    app.repositories = vec![Repository {
                        name: "test".into(),
                        path: "/test".into(),
                    }];
                    app.selected_repository = 0;
                    app.skip_push_confirmation =
                        skip && operation == crate::remotes::Operation::Push;
                    app.preferences.skip_sync_confirmation =
                        skip && operation == crate::remotes::Operation::Sync;
                    app.network_dialog = Some(RemoteDialog {
                        operation,
                        state: Some(crate::remotes::State {
                            remotes: vec!["origin".into()],
                            branch: Some("main".into()),
                            upstream_remote: Some("origin".into()),
                            upstream_ref: Some("main".into()),
                            ..Default::default()
                        }),
                        remote: "origin".into(),
                        branch: "main".into(),
                        set_upstream: false,
                        dont_show_again: false,
                        pull: Default::default(),
                    });
                    assert_eq!(app.remote_confirmation_hidden(), skip);
                    deliver(
                        &mut app,
                        WorkerResult::Operation {
                            committed: false,
                            success: operation.success(),
                            outcome: if failed {
                                Err("Rejected".into())
                            } else {
                                Ok(())
                            },
                            snapshot: if refresh_failed {
                                Err("Refresh failed".into())
                            } else {
                                Ok(git::Snapshot {
                                    repository: Repository {
                                        name: "test".into(),
                                        path: "/test".into(),
                                    },
                                    branch: "main".into(),
                                    changes: vec![],
                                })
                            },
                        },
                    );
                    assert_eq!(app.network_dialog.is_some(), !skip && failed);
                    assert_eq!(app.error.is_some(), failed || refresh_failed);
                    if !failed {
                        assert!(app.status.starts_with(operation.success()));
                        assert!(!app.status.contains("Push failed"));
                    }
                }
            }
        }
    }
}

#[test]
fn network_dialogs_keep_geometry_through_loading_errors_retry_and_cancellation() {
    use eframe::egui::{Context, Id, Pos2, RawInput};
    for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
        for style in [InterfaceStyle::Classic, InterfaceStyle::Modern] {
            for size in [egui::vec2(1100.0, 700.0), egui::vec2(1536.0, 1024.0)] {
                for operation in [
                    crate::remotes::Operation::Sync,
                    crate::remotes::Operation::Fetch,
                    crate::remotes::Operation::Pull,
                    crate::remotes::Operation::Push,
                ] {
                    let ctx = Context::default();
                    crate::theme::install_fonts(&ctx);
                    egui_extras::install_image_loaders(&ctx);
                    theme.apply(&ctx);
                    let mut app = SannyGitApp::with_theme(theme);
                    app.interface_style = style;
                    app.network_dialog = Some(RemoteDialog {
                        operation,
                        state: None,
                        remote: String::new(),
                        branch: String::new(),
                        set_upstream: false,
                        dont_show_again: false,
                        pull: Default::default(),
                    });
                    let mut baseline = None;
                    let mut button_positions = None;
                    let (sender, receiver) = mpsc::channel();
                    app.worker = Some(receiver);
                    for stage in 0..6 {
                        match stage {
                            1 => {
                                app.worker = None;
                                let dialog = app.network_dialog.as_mut().unwrap();
                                dialog.remote = "origin".into();
                                dialog.branch = "main".into();
                                dialog.state = Some(crate::remotes::State {
                                    remotes: vec!["origin".into()],
                                    branch: Some("main".into()),
                                    ..Default::default()
                                });
                            }
                            2 => {
                                app.error = Some("Network request failed. ".repeat(100));
                            }
                            3 => {
                                app.error = None;
                                let (_sender, receiver) = mpsc::channel();
                                app.worker = Some(receiver);
                                app.manager.cancel = Some(Arc::new(AtomicBool::new(false)));
                                app.status = "Network operation in progress…".into();
                            }
                            4 => {
                                app.cancel_repository_job();
                                assert!(app.busy());
                                assert!(!app.remote_allowed());
                                assert!(app
                                    .manager
                                    .cancel
                                    .as_ref()
                                    .unwrap()
                                    .load(Ordering::Relaxed));
                            }
                            5 => {
                                app.worker = None;
                                app.manager.cancel = None;
                                app.error = Some("Operation cancelled; verify local and remote state before retrying".into());
                            }
                            _ => {}
                        }
                        for frame in 0..4 {
                            let mut output = ctx.run_ui(
                                RawInput {
                                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                                    ..Default::default()
                                },
                                |ui| ui::show(ui, &mut app),
                            );
                            output.textures_delta.clear();
                            if frame < 3 {
                                continue;
                            }
                            if stage == 1 && operation == crate::remotes::Operation::Pull {
                                let labels = output
                                    .shapes
                                    .iter()
                                    .filter_map(|shape| match &shape.shape {
                                        egui::Shape::Text(text) => Some(text),
                                        _ => None,
                                    })
                                    .collect::<Vec<_>>();
                                for expected in [
                                    "Pull commits from a remote repository",
                                    "Fetch From:",
                                    "More Options",
                                    "Fetch Only",
                                ] {
                                    assert!(
                                        labels.iter().any(|text| text.galley.job.text == expected),
                                        "Missing Pull control: {expected}"
                                    );
                                }
                                for removed in [
                                    "Local branch:",
                                    "Remote branch:",
                                    "Cancellation does not roll back",
                                ] {
                                    assert!(!labels.iter().any(|text| text
                                        .galley
                                        .job
                                        .text
                                        .contains(removed)));
                                }
                            }
                            let rect = ctx
                                .memory(|memory| memory.area_rect(Id::unique("remote.operation")))
                                .unwrap();
                            assert!(Rect::from_min_size(Pos2::ZERO, size).contains_rect(rect));
                            if let Some(before) = baseline {
                                assert_eq!(rect, before, "Dialog changed at stage {stage}: {operation:?} {theme:?} {style:?} {size:?}");
                            } else {
                                baseline = Some(rect);
                            }
                            let buttons = output
                                .shapes
                                .iter()
                                .filter_map(|shape| match &shape.shape {
                                    egui::Shape::Text(text)
                                        if text.galley.job.text == "Cancel"
                                            || text.galley.job.text == operation.label() =>
                                    {
                                        Some((text.galley.job.text.clone(), text.pos))
                                    }
                                    _ => None,
                                })
                                .collect::<Vec<_>>();
                            if let Some(before) = &button_positions {
                                assert_eq!(
                                    &buttons, before,
                                    "Buttons moved at stage {stage}: {operation:?}"
                                );
                            } else {
                                button_positions = Some(buttons);
                            }
                        }
                    }
                    drop(sender);
                }
            }
        }
    }
}

#[test]
fn operation_outcomes_remain_distinct_from_refresh_and_cancellation() {
    for success in [
        "Commit created",
        "Commit amended",
        "Fetch complete",
        "Pull complete",
        "Push complete",
    ] {
        for failed in [false, true] {
            for refresh_failed in [false, true] {
                for cancelled in [false, true] {
                    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
                    app.repositories = crate::mock_data::repositories();
                    app.selected_repository = 0;
                    app.changes = crate::mock_data::changes();
                    app.selected_change = 2;
                    app.commits = crate::mock_data::commits();
                    app.selected_commit = Some(app.commits[1].hash.clone());
                    let selected_commit = app.selected_commit.clone();
                    let selected_path = app.changes[2].path.clone();
                    app.commit_message = "Keep on failure".into();
                    app.state_current = false;
                    app.manager.cancel = Some(Arc::new(AtomicBool::new(false)));
                    if cancelled {
                        app.cancel_repository_job();
                    }
                    let snapshot = if refresh_failed {
                        Err("Refresh diagnostic".into())
                    } else {
                        Ok(git::Snapshot {
                            repository: Repository {
                                name: app.repositories[0].name.clone(),
                                path: app.repositories[0].path.clone(),
                            },
                            branch: "main".into(),
                            changes: crate::mock_data::changes(),
                        })
                    };
                    deliver(
                        &mut app,
                        WorkerResult::Operation {
                            committed: !failed && success.starts_with("Commit"),
                            success,
                            outcome: if failed {
                                Err("Operation diagnostic".into())
                            } else {
                                Ok(())
                            },
                            snapshot,
                        },
                    );
                    assert_eq!(app.state_current, !refresh_failed);
                    assert_eq!(app.selected_commit, selected_commit);
                    assert_eq!(app.changes[app.selected_change].path, selected_path);
                    assert_eq!(
                        app.commit_message.is_empty(),
                        !failed && success.starts_with("Commit")
                    );
                    if failed {
                        assert!(app
                            .error
                            .as_deref()
                            .unwrap()
                            .contains("Operation diagnostic"));
                        if cancelled {
                            assert!(app.status.starts_with("Cancellation requested"));
                            assert!(app.error.as_deref().unwrap().contains("does not roll back"));
                        } else {
                            assert!(app.status.starts_with("Operation failed"));
                        }
                    } else {
                        assert!(app.status.starts_with(success));
                    }
                    if refresh_failed {
                        assert!(app.error.as_deref().unwrap().contains("Refresh diagnostic"));
                        assert!(app.status.contains("refresh required"));
                    } else if !failed {
                        assert!(app.error.is_none());
                    }
                }
            }
        }
    }
}

#[test]
fn operation_errors_keep_draft_and_success_restores_normal_draft_even_if_refresh_fails() {
    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
    app.commit_message = "Keep draft".into();
    app.amend = true;
    app.amend_head = Some("a".repeat(40));
    app.amend_draft = Some("Original draft".into());
    app.state_current = false;
    deliver(
        &mut app,
        WorkerResult::Operation {
            committed: false,
            success: "Index updated",
            outcome: Err("Hook rejected commit".into()),
            snapshot: Err("Refresh failed".into()),
        },
    );
    assert_eq!(app.commit_message, "Keep draft");
    assert!(app.amend);
    assert!(app.amend_head.is_some());
    assert!(app.error.as_deref().unwrap().contains("Hook"));
    assert!(!app.state_current);
    deliver(
        &mut app,
        WorkerResult::Operation {
            committed: true,
            success: "Commit created",
            outcome: Ok(()),
            snapshot: Err("Refresh failed".into()),
        },
    );
    assert_eq!(app.commit_message, "Original draft");
    assert!(!app.amend);
    assert!(app.amend_head.is_none());
    assert!(app.amend_draft.is_none());
    assert!(app.error.as_deref().unwrap().contains("refresh"));
    assert!(!app.state_current);
    assert!(app.commit_blocker().is_some());
}

#[test]
fn history_errors_retain_rows_and_late_commit_previews_are_discarded() {
    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
    app.repositories = crate::mock_data::repositories();
    app.selected_repository = 0;
    app.commits = crate::mock_data::commits();
    app.state_current = true;
    let selected = app.commits[0].hash.clone();
    app.select_commit(selected.clone());
    let path = app.repositories[0].path.clone();
    let count = app.commits.len();
    let preview_path = app.repositories[0].path.clone();
    let preview_generation = app.commit_preview_generation;
    app.selected_commit_file = Some("root".into());
    deliver(
        &mut app,
        WorkerResult::History {
            path: "other-repository".into(),
            all: false,
            generation: 0,
            append: false,
            result: Err("Late history result".into()),
        },
    );
    assert_eq!(app.commits.len(), count);
    deliver(
        &mut app,
        WorkerResult::History {
            path,
            all: false,
            generation: 0,
            append: false,
            result: Err("Cannot read history".into()),
        },
    );
    assert_eq!(app.commits.len(), count);
    assert!(app.history_error.is_some());
    assert!(app.state_current);
    deliver(
        &mut app,
        WorkerResult::CommitPatch {
            path: preview_path.clone(),
            file: "root".into(),
            generation: preview_generation,
            hash: "old selection".into(),
            result: Err("Stale preview".into()),
        },
    );
    assert!(app.commit_preview_error.is_none());
    assert!(app.commit_preview.is_none());
    deliver(
        &mut app,
        WorkerResult::CommitPatch {
            path: preview_path.clone(),
            file: "root".into(),
            generation: preview_generation,
            hash: selected,
            result: Ok(crate::models::DiffPreview::new(
                "@@ -0,0 +1 @@\n+commit preview",
            )),
        },
    );
    assert_eq!(app.commit_preview.as_ref().unwrap().added, 1);
}

#[test]
fn rename_requires_fresh_local_target_and_retains_draft_on_failure() {
    use crate::menu::MenuAction;
    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
    app.repositories = crate::mock_data::repositories();
    app.selected_repository = 0;
    app.refs = crate::mock_data::refs();
    app.selected_branch = "refs/heads/develop".into();
    app.state_current = true;
    assert!(!app.menu_action_enabled(MenuAction::RenameBranch));
    app.refs_current = true;
    assert!(app.menu_action_enabled(MenuAction::RenameBranch));
    app.open_rename_branch();
    assert!(app.rename_branch_blocker().is_some()); // unchanged name
    app.rename_branch_dialog.as_mut().unwrap().name = "renamed".into();
    app.changes = crate::mock_data::changes();
    assert!(app.rename_branch_blocker().is_none()); // dirty files are allowed
    app.selected_branch = "refs/tags/v1.0.0".into();
    assert!(!app.menu_action_enabled(MenuAction::RenameBranch));
    assert!(app.rename_branch_blocker().is_none()); // dialog keeps its source
    deliver(
        &mut app,
        WorkerResult::Operation {
            committed: false,
            success: "Branch renamed",
            outcome: Err("Cannot rename".into()),
            snapshot: Err("Refresh failed".into()),
        },
    );
    assert_eq!(app.rename_branch_dialog.as_ref().unwrap().name, "renamed");
    assert!(app.rename_branch_blocker().is_some());
    deliver(
        &mut app,
        WorkerResult::Operation {
            committed: false,
            success: "Branch renamed",
            outcome: Ok(()),
            snapshot: Err("Refresh failed".into()),
        },
    );
    assert!(app.rename_branch_dialog.is_none());
    assert_eq!(app.selected_branch, "refs/heads/renamed");
}

#[test]
fn tag_creation_requires_fresh_state_and_keeps_draft_on_failure() {
    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
    app.repositories = crate::mock_data::repositories();
    app.selected_repository = 0;
    app.new_tag_name = "release/new".into();
    app.new_tag_message = "Release details".into();
    app.tag_dialog_open = true;
    app.state_current = true;
    assert!(app.create_tag_blocker().is_some());
    app.refs_current = true;
    assert!(app.create_tag_blocker().is_some());
    app.branch_head_exists = true;
    app.changes = crate::mock_data::changes();
    assert!(app.create_tag_blocker().is_none());
    deliver(
        &mut app,
        WorkerResult::Operation {
            committed: false,
            success: "Tag created",
            outcome: Err("Tag exists".into()),
            snapshot: Err("Refresh failed".into()),
        },
    );
    assert!(app.tag_dialog_open);
    assert_eq!(app.new_tag_name, "release/new");
    assert_eq!(app.new_tag_message, "Release details");
    assert!(app.create_tag_blocker().is_some());
    deliver(
        &mut app,
        WorkerResult::Operation {
            committed: false,
            success: "Tag created",
            outcome: Ok(()),
            snapshot: Err("Refresh failed".into()),
        },
    );
    assert!(!app.tag_dialog_open);
    assert!(app.new_tag_name.is_empty());
    assert!(app.new_tag_message.is_empty());
    assert_eq!(app.selected_branch, "refs/tags/release/new");
}

#[test]
fn branch_actions_require_fresh_refs_and_clean_switches() {
    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
    app.repositories = crate::mock_data::repositories();
    app.selected_repository = 0;
    app.refs = crate::mock_data::refs();
    app.selected_branch = "refs/heads/develop".into();
    app.state_current = true;
    app.branch_head_exists = true;
    app.new_branch_name = "feature/new".into();
    assert!(app.switch_branch_blocker().is_some());
    app.refs_current = true;
    assert!(app.switch_branch_blocker().is_none());
    assert!(app.create_branch_blocker().is_none());
    app.changes = crate::mock_data::changes();
    assert!(app.switch_branch_blocker().is_some());
    assert!(app.create_branch_blocker().is_some());
    app.switch_branch();
    app.create_branch();
    assert!(!app.busy());
    app.switch_to_new_branch = false;
    assert!(app.create_branch_blocker().is_none());
    app.branch_head_exists = false;
    assert!(app
        .create_branch_blocker()
        .unwrap()
        .contains("first commit"));
    app.changes.clear();
    app.selected_branch = "refs/tags/v1.0.0".into();
    assert!(app.switch_branch_blocker().is_none());
}

#[test]
fn remote_checkout_uses_current_tracking_branch_or_shows_choices() {
    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
    app.repositories = crate::mock_data::repositories();
    app.selected_repository = 0;
    app.refs = crate::mock_data::refs();
    app.state_current = true;
    app.refs_current = true;
    let remote = app
        .refs
        .iter()
        .find(|r| r.kind == crate::models::RefKind::Remote)
        .unwrap();
    let remote_name = remote.name.clone();
    app.selected_branch = remote.full_name.clone();
    let local = app
        .refs
        .iter_mut()
        .find(|r| r.kind == crate::models::RefKind::Local)
        .unwrap();
    local.upstream = Some(remote_name);
    local.current = true;
    let local_ref = local.full_name.clone();
    app.switch_branch();
    assert_eq!(app.selected_branch, local_ref);
    assert!(app.remote_checkout.is_none() && !app.busy());
    app.selected_branch = "refs/remotes/origin/new-topic".into();
    app.refs.push(crate::models::BranchRef {
        full_name: app.selected_branch.clone(),
        name: "origin/new-topic".into(),
        hash: "a".repeat(40),
        upstream: None,
        kind: crate::models::RefKind::Remote,
        current: false,
    });
    app.switch_branch();
    let dialog = app.remote_checkout.as_ref().unwrap();
    assert_eq!(dialog.new_name, "new-topic");
    assert_eq!(dialog.mode, 1);
    assert!(!app.busy());
}

#[test]
fn branch_load_errors_keep_rows_but_disable_branch_actions() {
    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
    app.repositories = crate::mock_data::repositories();
    app.selected_repository = 0;
    app.refs = crate::mock_data::refs();
    app.state_current = true;
    app.refs_current = true;
    let count = app.refs.len();
    deliver(
        &mut app,
        WorkerResult::Refs {
            path: "other repo".into(),
            result: Ok(crate::models::RefSnapshot {
                refs: Vec::new(),
                has_head: false,
            }),
        },
    );
    assert_eq!(app.refs.len(), count);
    assert!(app.refs_current);
    let path = app.repositories[0].path.clone();
    deliver(
        &mut app,
        WorkerResult::Refs {
            path,
            result: Err("Cannot read refs".into()),
        },
    );
    assert_eq!(app.refs.len(), count);
    assert!(app.state_current);
    assert!(!app.refs_current);
    assert!(app.refs_error.is_some());
    assert!(app.switch_branch_blocker().is_some());
    assert!(app.create_branch_blocker().is_some());
}

fn visible_text_count(shapes: &[ClippedShape], expected: &str) -> usize {
    shapes
        .iter()
        .filter(|shape| {
            if let egui::Shape::Text(text) = &shape.shape {
                text.galley.job.text == expected
                    && shape
                        .clip_rect
                        .intersect(text.galley.rect.translate(text.pos.to_vec2()))
                        .height()
                        > 8.0
            } else {
                false
            }
        })
        .count()
}

fn visible_text(shapes: &[ClippedShape], expected: &str) -> bool {
    visible_text_count(shapes, expected) > 0
}

#[test]
fn welcome_waits_for_restore_and_only_opens_without_an_active_repository() {
    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
    app.welcome_pending = true;
    let (_sender, receiver) = mpsc::channel();
    app.worker = Some(receiver);
    app.finish_startup();
    assert!(app.welcome_pending);
    assert!(!app.welcome_open);
    app.worker = None;
    app.repositories = crate::mock_data::repositories();
    app.selected_repository = 0;
    app.finish_startup();
    assert!(!app.welcome_open);
    app.welcome_pending = true;
    app.selected_repository = usize::MAX;
    app.finish_startup();
    assert!(app.welcome_open);
    app.welcome_open = false;
    app.finish_startup();
    assert!(!app.welcome_open, "Welcome must not reopen after closing");
    app.welcome_pending = true;
    app.preferences.show_welcome = false;
    app.finish_startup();
    assert!(!app.welcome_open);
}

#[test]
fn double_click_staging_setting_changes_only_the_index() {
    use crate::preferences::FileDoubleClick;
    use egui::{Event, Modifiers, PointerButton};
    for mode in [FileDoubleClick::ShowDiff, FileDoubleClick::ToggleStage] {
        let repo = crate::test_support::TempRepo::new();
        std::fs::write(repo.0.join("example.txt"), "Keep working-tree content\n").unwrap();
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.apply_snapshot(git::load(&repo.0).unwrap());
        app.preferences.file_double_click = mode;
        let context = egui::Context::default();
        crate::theme::install_fonts(&context);
        egui_extras::install_image_loaders(&context);
        ThemeChoice::Light.apply(&context);
        let render = |app: &mut SannyGitApp, time, events| {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(1100.0, 700.0),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| ui::show(ui, app),
            );
            output.textures_delta.clear();
            output
        };
        for round in 0..2 {
            let start = 1.0 + round as f64;
            let mut output = render(&mut app, start - 0.5, vec![]);
            for _ in 0..2 {
                output = render(&mut app, start - 0.4, vec![]);
            }
            let target = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.job.text == "example.txt" => {
                        Some(text.pos + text.galley.size() / 2.0)
                    }
                    _ => None,
                })
                .unwrap();
            for (step, pressed) in [true, false, true, false].into_iter().enumerate() {
                render(
                    &mut app,
                    start + step as f64 * 0.05,
                    vec![
                        Event::PointerMoved(target),
                        Event::PointerButton {
                            pos: target,
                            button: PointerButton::Primary,
                            pressed,
                            modifiers: Modifiers::NONE,
                        },
                    ],
                );
            }
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while app.busy() && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(5));
                app.poll_worker();
            }
            assert!(!app.busy());
            assert!(app.error.is_none(), "{:?}", app.error);
            let snapshot = git::load(&repo.0).unwrap();
            assert_eq!(
                snapshot.changes[0].staged,
                mode == FileDoubleClick::ToggleStage && round == 0
            );
            assert_eq!(
                std::fs::read_to_string(repo.0.join("example.txt")).unwrap(),
                "Keep working-tree content\n"
            );
        }
    }
}

#[test]
fn modern_layout_fits_both_sizes_and_color_themes_without_loading_jumps() {
    for size in [Vec2::new(1100.0, 700.0), Vec2::new(1536.0, 1024.0)] {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            let context = egui::Context::default();
            crate::theme::install_fonts(&context);
            egui_extras::install_image_loaders(&context);
            theme.apply_style(&context, InterfaceStyle::Modern);
            let mut app = SannyGitApp::with_theme(theme);
            app.interface_style = InterfaceStyle::Modern;
            app.repositories = crate::mock_data::repositories();
            app.changes = crate::mock_data::changes();
            app.commits = crate::mock_data::commits();
            app.refs = crate::mock_data::refs();
            app.current_branch = "main".into();
            app.selected_change = 2;
            app.loaded_diff = Some(2);
            app.diff_preview = Some(crate::models::DiffPreview::new(&app.changes[2].diff));
            let render = |app: &mut SannyGitApp| {
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ui| ui::show(ui, app),
                );
                output.textures_delta.clear();
                output
            };
            for _ in 0..3 {
                render(&mut app);
            }
            let output = render(&mut app);
            for label in [
                "Repositories",
                "Branches",
                "Settings",
                "Commit",
                "Journal",
                "diff_view.rs",
                "Initial commit",
            ] {
                assert!(
                    visible_text(&output.shapes, label),
                    "Missing {label} at {size:?} in {theme:?}"
                );
            }
            let label_rect = |label: &str| {
                output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.job.text == label => {
                            Some(text.galley.rect.translate(text.pos.to_vec2()))
                        }
                        _ => None,
                    })
                    .unwrap()
            };
            assert!(label_rect("diff_view.rs").left() > label_rect("File").right());
            assert!(label_rect("Journal").left() > label_rect("Commit").right());
            assert!(label_rect("Commit").top() > label_rect("File").bottom());
            let panels = || {
                context.data(|data| {
                    let mut rects: Vec<_> = data
                        .iter_temp_by_type::<egui::PanelState>()
                        .map(|panel| panel.outer_rect)
                        .collect();
                    rects.sort_by(|a, b| {
                        a.left()
                            .total_cmp(&b.left())
                            .then(a.top().total_cmp(&b.top()))
                    });
                    rects
                })
            };
            let before = panels();
            app.changes.clear();
            app.commits.clear();
            app.refs_loading = true;
            app.history_loading = true;
            for _ in 0..3 {
                render(&mut app);
            }
            assert_eq!(
                before,
                panels(),
                "Loading moved panels at {size:?} in {theme:?}"
            );
        }
    }
}

#[test]
fn empty_state_and_open_dialog_fit_in_both_themes() {
    for size in [Vec2::new(1100.0, 700.0), Vec2::new(1536.0, 1024.0)] {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            let context = egui::Context::default();
            crate::theme::install_fonts(&context);
            egui_extras::install_image_loaders(&context);
            theme.apply(&context);
            let mut app = SannyGitApp::with_theme(theme);
            app.open_dialog = true;
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            };
            let mut output = context.run_ui(input.clone(), |ui| ui::show(ui, &mut app));
            output.textures_delta.clear();
            for _ in 0..2 {
                output = context.run_ui(input.clone(), |ui| ui::show(ui, &mut app));
                output.textures_delta.clear();
            }
            for text in ["OK", "Settings", "Add existing or create new repository"] {
                assert!(
                    visible_text(&output.shapes, text),
                    "{text} clipped at {size:?} in {theme:?}"
                );
            }
            assert!(app.repositories.is_empty());
            assert!(app.worker.is_none());
        }
    }
}

#[test]
fn all_changed_files_fit_at_reference_and_small_window_sizes() {
    for size in [
        Vec2::new(1536.0, 1024.0),
        Vec2::new(1244.0, 720.0),
        Vec2::new(1100.0, 700.0),
    ] {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            let context = egui::Context::default();
            crate::theme::install_fonts(&context);
            egui_extras::install_image_loaders(&context);
            theme.apply(&context);
            let mut app = SannyGitApp::with_theme(theme);
            app.repositories = crate::mock_data::repositories();
            app.changes = crate::mock_data::changes();
            let history = crate::history::prepare(crate::mock_data::commits(), false);
            app.commits = history.commits;
            app.history_edges = history.edges;
            app.history_max_lane = history.max_lane;
            app.refs = crate::mock_data::refs();
            app.refs_current = true;
            app.branch_head_exists = true;
            app.selected_repository = 0;
            app.state_current = true;
            app.commit_message = "Layout check".into();
            app.selected_change = 2;
            app.loaded_diff = Some(2);
            app.diff_preview = Some(crate::models::DiffPreview::new(&app.changes[2].diff));
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            };
            let mut output = context.run_ui(input.clone(), |ui| ui::show(ui, &mut app));
            // Headless layout checks do not upload textures to a renderer.
            output.textures_delta.clear();
            for _ in 0..2 {
                output = context.run_ui(input.clone(), |ui| ui::show(ui, &mut app));
                output.textures_delta.clear();
            }
            for file in &app.changes {
                assert!(
                    visible_text(&output.shapes, &file.path),
                    "File {} is clipped at {size:?} in {theme:?}",
                    file.path
                );
            }
            assert!(
                visible_text_count(&output.shapes, "Commit") >= 2,
                "Commit control is clipped at {size:?}"
            );
            assert!(
                visible_text(&output.shapes, "Initial commit"),
                "History is clipped at {size:?}"
            );
            app.select_commit(app.commits.last().unwrap().hash.clone());
            app.commit_preview = Some(crate::models::DiffPreview::new(
                "@@ -0,0 +1 @@\n+Read-only commit preview",
            ));
            for _ in 0..3 {
                output = context.run_ui(input.clone(), |ui| ui::show(ui, &mut app));
                output.textures_delta.clear();
            }
            for text in [
                "Commit details",
                "Copy hash",
                "Commit changes",
                "Read-only commit preview",
            ] {
                assert!(
                    visible_text(&output.shapes, text),
                    "Commit details {text} clipped at {size:?} in {theme:?}"
                );
            }
            assert!(
                app.worker.is_none(),
                "Rendering history must not run Git commands"
            );
            app.commit_details_open = false;
            app.current_branch = "main".into();
            app.branch_dialog_open = true;
            app.new_branch_name = "feature/new-ui".into();
            for _ in 0..3 {
                output = context.run_ui(input.clone(), |ui| ui::show(ui, &mut app));
                output.textures_delta.clear();
            }
            for text in ["Create branch", "Switch to the new branch", "Create"] {
                assert!(
                    visible_text(&output.shapes, text),
                    "Branch control {text} clipped at {size:?} in {theme:?}"
                );
            }
            assert!(
                app.worker.is_none(),
                "Rendering branch controls must not run Git commands"
            );
            app.branch_dialog_open = false;
            app.remote_checkout = Some(RemoteCheckoutDialog {
                remote: "refs/remotes/origin/topic".into(),
                mode: 1,
                local: "refs/heads/main".into(),
                new_name: "topic".into(),
            });
            for _ in 0..3 {
                output = context.run_ui(input.clone(), |ui| ui::show(ui, &mut app));
                output.textures_delta.clear();
            }
            for label in [
                "Check out commit",
                "Checkout local branch:",
                "Create local branch:",
                "Checkout",
                "Cancel",
            ] {
                assert!(
                    visible_text(&output.shapes, label),
                    "Remote checkout {label} clipped at {size:?} in {theme:?}"
                );
            }
            app.remote_checkout = None;
            for operation in [
                crate::remotes::Operation::Sync,
                crate::remotes::Operation::Fetch,
                crate::remotes::Operation::Pull,
                crate::remotes::Operation::Push,
            ] {
                app.network_dialog = Some(remotes::RemoteDialog {
                    operation,
                    state: Some(crate::remotes::State {
                        remotes: vec!["origin".into()],
                        branch: Some("main".into()),
                        ..Default::default()
                    }),
                    remote: "origin".into(),
                    branch: "main".into(),
                    set_upstream: false,
                    dont_show_again: false,
                    pull: Default::default(),
                });
                for _ in 0..3 {
                    output = context.run_ui(input.clone(), |ui| ui::show(ui, &mut app));
                    output.textures_delta.clear();
                }
                let destination_label = if matches!(
                    operation,
                    crate::remotes::Operation::Push | crate::remotes::Operation::Sync
                ) {
                    "Options"
                } else if operation == crate::remotes::Operation::Pull {
                    "Fetch From:"
                } else {
                    "Remote:"
                };
                for label in [destination_label, "Cancel"] {
                    assert!(
                        visible_text(&output.shapes, label),
                        "Remote operation {label} clipped at {size:?} in {theme:?}"
                    );
                }
                if operation == crate::remotes::Operation::Pull {
                    assert!(visible_text(&output.shapes, "Fetch Only"));
                    assert!(visible_text(&output.shapes, "More Options"));
                }
                assert!(app.worker.is_none());
            }
            app.network_dialog = None;
            app.manager.settings = Some(crate::repository::Settings::default());
            app.manager.prune_preview = Some("Preview of obsolete metadata".into());
            app.manager.found = vec![("Found repo".into(), "/tmp/found-repo".into(), true)];
            app.manager.worktrees = vec![crate::repository::Worktree {
                path: "/tmp/linked-tree".into(),
                branch: Some("feature".into()),
                main: false,
                locked: false,
                prunable: false,
                bare: false,
            }];
            app.manager.selected_worktree = Some("/tmp/linked-tree".into());
            app.manager.confirm_remove = true;
            for (dialog, button) in [
                (RepositoryDialog::Group, "Add Group"),
                (RepositoryDialog::Clone, "Continue"),
                (RepositoryDialog::Search, "Add Selected"),
                (RepositoryDialog::Worktrees, "Remove Worktree"),
                (RepositoryDialog::Prune, "Prune Obsolete Metadata"),
                (RepositoryDialog::Settings, "OK"),
                (RepositoryDialog::CloseRepository, "Close Repository"),
            ] {
                app.manager.dialog = Some(dialog);
                for _ in 0..3 {
                    output = context.run_ui(input.clone(), |ui| ui::show(ui, &mut app));
                    output.textures_delta.clear();
                }
                assert!(
                    visible_text(&output.shapes, button),
                    "Repository control {button} clipped at {size:?} in {theme:?}"
                );
                assert!(
                    app.worker.is_none(),
                    "Rendering repository dialogs must not execute Git operations"
                );
            }
            app.manager.dialog = Some(RepositoryDialog::Clone);
            app.manager.clone_branches = vec!["main".into(), "feature".into()];
            app.manager.clone_options.branch = "main".into();
            for (step, label) in [(1, "Check Out Branch:"), (2, "Finish")] {
                app.manager.clone_step = step;
                for _ in 0..3 {
                    output = context.run_ui(input.clone(), |ui| ui::show(ui, &mut app));
                    output.textures_delta.clear();
                }
                assert!(
                    visible_text(&output.shapes, label),
                    "Clone step {step} clipped at {size:?} in {theme:?}"
                );
                assert!(app.worker.is_none());
            }
        }
    }
}

#[test]
fn settings_preserve_unloaded_keys_and_only_write_explicit_secret_changes() {
    let previous = crate::llm::Config::default();
    let mut draft = LlmSettings {
        config: previous.clone(),
        ..Default::default()
    };
    assert!(!draft.secret_changed(&previous));
    draft.config.model = "new-model".into();
    draft.config.instructions = "New instructions".into();
    draft.config.endpoint = "https://other.example/v1".into();
    assert!(!draft.secret_changed(&previous));
    draft.save_secret(&previous, None).unwrap();

    draft.config.store_api_key = false;
    assert!(draft.secret_changed(&previous));
    let without_storage = draft.config.clone();
    draft.config.store_api_key = true;
    assert!(!draft.secret_changed(&without_storage));

    draft.config.api_key = "replacement".into();
    draft.api_key_dirty = true;
    assert!(draft.secret_changed(&previous));
    draft.config.api_key.clear();
    assert!(draft.secret_changed(&previous));
}

#[test]
fn commit_file_selection_retains_list_after_errors_and_rejects_stale_results() {
    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
    app.repositories = crate::mock_data::repositories();
    app.selected_repository = 0;
    app.commits = crate::mock_data::commits();
    let hash = app.commits[0].hash.clone();
    app.select_commit(hash.clone());
    let path = app.repositories[0].path.clone();
    let generation = app.commit_preview_generation;
    let list = vec![
        crate::history::CommitFile {
            status: "A".into(),
            path: "large".into(),
            original_path: None,
        },
        crate::history::CommitFile {
            status: "M".into(),
            path: "small".into(),
            original_path: None,
        },
    ];
    deliver(
        &mut app,
        WorkerResult::CommitFiles {
            path: path.clone(),
            hash: hash.clone(),
            generation,
            result: Ok(list.clone()),
        },
    );
    assert_eq!(app.selected_commit_file.as_deref(), Some("large"));
    deliver(
        &mut app,
        WorkerResult::CommitPatch {
            path: path.clone(),
            hash: hash.clone(),
            generation,
            file: "large".into(),
            result: Err("Diff exceeds limit".into()),
        },
    );
    app.collect_errors();
    assert_eq!(
        app.error_dialogs.pending.back().unwrap(),
        "Diff exceeds limit"
    );
    app.select_commit_file("small".into());
    assert_eq!(app.commit_files.as_ref().unwrap(), &list);
    assert!(app.commit_preview_error.is_none());
    let current = app.commit_preview_generation;
    for (worker_path, worker_generation, file) in [
        (path.clone(), generation, "large"),
        ("other repository".into(), current, "small"),
        (path.clone(), current, "large"),
    ] {
        deliver(
            &mut app,
            WorkerResult::CommitPatch {
                path: worker_path,
                hash: hash.clone(),
                generation: worker_generation,
                file: file.into(),
                result: Err("Stale diff".into()),
            },
        );
        assert!(app.commit_preview_error.is_none());
    }
    deliver(
        &mut app,
        WorkerResult::CommitPatch {
            path: path.clone(),
            hash: hash.clone(),
            generation: current,
            file: "small".into(),
            result: Ok(crate::models::DiffPreview::new("@@ -0,0 +1 @@\n+small")),
        },
    );
    assert_eq!(app.commit_preview.as_ref().unwrap().added, 1);
    app.retry_commit_preview();
    assert_eq!(app.commit_files.as_ref().unwrap(), &list);
    assert!(app.commit_preview.is_some());
    app.select_commit(app.commits[1].hash.clone());
    deliver(
        &mut app,
        WorkerResult::CommitFiles {
            path,
            hash,
            generation: current,
            result: Ok(list),
        },
    );
    assert!(app.commit_files.is_none());
    assert!(app.commit_preview.is_none());
}

fn commit_message_result(result: Result<String, String>) -> WorkerResult {
    WorkerResult::CommitMessage {
        result,
        config: crate::llm::Config::default(),
        repository: None,
    }
}

#[test]
fn unlocked_generation_key_is_reused_after_success_and_network_failure_without_persisting_it() {
    for result in [
        Ok("Generated message".into()),
        Err("Provider unavailable".into()),
    ] {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        let config = crate::llm::Config {
            api_key: "unlocked-dummy-key".into(),
            ..app.llm_config.clone()
        };
        deliver(
            &mut app,
            WorkerResult::CommitMessage {
                result,
                config,
                repository: None,
            },
        );
        let mut next = app.effective_llm_config("/repo").clone();
        assert_eq!(next.api_key, "unlocked-dummy-key");
        crate::llm::CREDENTIAL_READS.with(|reads| reads.set(0));
        crate::llm::load_api_key_if_needed(&mut next, None).unwrap();
        crate::llm::load_api_key_if_needed(&mut next, None).unwrap();
        crate::llm::CREDENTIAL_READS.with(|reads| assert_eq!(reads.get(), 0));
        let saved = serde_json::to_string(&app.llm_config).unwrap();
        assert!(!saved.contains("unlocked-dummy-key"));
        let restored: crate::llm::Config = serde_json::from_str(&saved).unwrap();
        assert!(restored.api_key.is_empty());
    }
}

#[test]
fn unlocked_repository_key_stays_in_its_original_scope_after_selection_changes() {
    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
    app.repository_llm_configs
        .insert("/first".into(), app.llm_config.clone());
    app.repository_llm_configs
        .insert("/second".into(), app.llm_config.clone());
    let config = crate::llm::Config {
        api_key: "first-repository-dummy-key".into(),
        ..app.llm_config.clone()
    };
    deliver(
        &mut app,
        WorkerResult::CommitMessage {
            result: Err("Provider unavailable".into()),
            config,
            repository: Some("/first".into()),
        },
    );
    assert_eq!(
        app.effective_llm_config("/first").api_key,
        "first-repository-dummy-key"
    );
    assert!(app.effective_llm_config("/second").api_key.is_empty());
    assert!(app.llm_config.api_key.is_empty());
}

#[test]
fn late_unlock_does_not_restore_removed_or_changed_credentials() {
    for change in 0..4 {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        let config = crate::llm::Config {
            api_key: "old-unlocked-dummy-key".into(),
            ..app.llm_config.clone()
        };
        app.repository_llm_configs
            .insert("/repo".into(), app.llm_config.clone());
        match change {
            0 => {
                app.repository_llm_configs.remove("/repo");
            }
            1 => {
                app.repository_llm_configs
                    .get_mut("/repo")
                    .unwrap()
                    .endpoint = "https://other.invalid/v1".into()
            }
            2 => {
                app.repository_llm_configs
                    .get_mut("/repo")
                    .unwrap()
                    .store_api_key = false
            }
            _ => {
                app.repository_llm_configs.get_mut("/repo").unwrap().api_key =
                    "new-dummy-key".into()
            }
        }
        let expected = app.repository_llm_configs.clone();
        deliver(
            &mut app,
            WorkerResult::CommitMessage {
                result: Ok("Generated message".into()),
                config,
                repository: Some("/repo".into()),
            },
        );
        assert_eq!(app.repository_llm_configs, expected);
        assert!(app.llm_config.api_key.is_empty());
    }
}

#[test]
fn generation_reviews_a_fixed_snapshot_before_any_generation_request() {
    use std::{
        io::{BufRead, BufReader, Read, Write},
        net::TcpListener,
        time::{Duration, Instant},
    };
    let repo = crate::test_support::TempRepo::new();
    repo.write("review.txt", "reviewed contents\n");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
    app.apply_snapshot(git::load(&repo.0).unwrap());
    app.llm_config.endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    app.llm_config.store_api_key = false;
    app.commit_message = "Keep draft".into();
    app.generate_commit_message();
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.busy() {
        assert!(Instant::now() < deadline);
        app.poll_worker();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!app.generating_commit_message);
    assert_eq!(app.commit_message, "Keep draft");
    let patch = app
        .generation_review
        .as_ref()
        .unwrap()
        .patch
        .clone()
        .unwrap();
    assert!(patch.contains("reviewed contents"));
    assert!(
        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
    repo.write("review.txt", "later contents must not be sent\n");
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline);
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("{error}"),
            }
        };
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut reader = BufReader::new(&mut socket);
        let mut length = 0;
        loop {
            let mut line = String::new();
            assert!(reader.read_line(&mut line).unwrap() > 0);
            if line == "\r\n" {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse::<usize>().unwrap();
            }
        }
        let mut bytes = vec![0; length];
        reader.read_exact(&mut bytes).unwrap();
        let body = r#"{"choices":[{"message":{"content":"Reviewed message"}}]}"#;
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()
    });
    app.send_reviewed_commit_message();
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.busy() {
        assert!(Instant::now() < deadline);
        app.poll_worker();
        std::thread::sleep(Duration::from_millis(5));
    }
    let request = server.join().unwrap();
    assert_eq!(
        request["messages"][1]["content"],
        format!("Generate a commit message for these changes:\n\n{patch}")
    );
    assert_eq!(app.commit_message, "Reviewed message");
    assert_eq!(
        std::fs::read_to_string(repo.0.join("review.txt")).unwrap(),
        "later contents must not be sent\n"
    );
}

#[test]
fn cancelled_failed_or_wrong_repository_reviews_cannot_send_or_replace_a_draft() {
    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
    app.repositories = crate::mock_data::repositories();
    app.selected_repository = 0;
    app.commit_message = "Keep draft".into();
    let review = || GenerationReview {
        root: "/different-repository".into(),
        config: crate::llm::Config::default(),
        repository_scope: None,
        patch: Some("reviewed patch".into()),
        preview: None,
    };
    app.generation_review = Some(review());
    app.send_reviewed_commit_message();
    assert!(!app.busy());
    app.generation_review = None;
    deliver(
        &mut app,
        WorkerResult::GenerationPreview(Ok((
            crate::llm::Config::default(),
            "late patch".into(),
            crate::models::DiffPreview::new("late patch"),
        ))),
    );
    assert!(app.generation_review.is_none());
    app.send_reviewed_commit_message();
    assert!(!app.busy());
    app.generation_review = Some(GenerationReview {
        patch: None,
        ..review()
    });
    deliver(
        &mut app,
        WorkerResult::GenerationPreview(Err("Snapshot failed".into())),
    );
    assert_eq!(app.error.as_deref(), Some("Snapshot failed"));
    app.collect_errors();
    assert!(app
        .error_dialogs
        .pending
        .iter()
        .any(|error| error == "Snapshot failed"));
    assert!(app.generation_review.as_ref().unwrap().patch.is_none());
    app.send_reviewed_commit_message();
    assert!(!app.busy());
    assert_eq!(app.commit_message, "Keep draft");
}
