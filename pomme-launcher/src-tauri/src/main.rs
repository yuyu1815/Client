#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::Manager;

fn main() {
    #[cfg(target_os = "linux")]
    if std::env::var("WEBKIT_DISABLE_COMPOSITING_MODE").is_err() {
        unsafe { std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "0") };
    }

    // Opt-in only: a normal launcher invocation never touches benchmark state.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let benchmark = if args.first().is_some_and(|a| a == "--auto-benchmark") {
        if !(args.len() == 2 || args.len() == 3) {
            println!("usage: --auto-benchmark <server> [account-id]");
            std::process::exit(2);
        }
        Some((args[1].clone(), args.get(2).cloned()))
    } else {
        None
    };

    let builder = pomme_launcher::get_builder();

    #[cfg(debug_assertions)]
    pomme_launcher::generate_bindings();

    let invoke_handler = builder.invoke_handler();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            builder.mount_events(app);
            pomme_launcher::storage::ensure_dirs();
            app.manage(pomme_launcher::AppState::default());
            if let Some((server, account_id)) = benchmark.as_ref() {
                // Keep the launcher out of the way; the client/game window remains visible.
                if let Some(window) = app.get_webview_window("main") {
                    window.hide()?;
                }
                let handle = app.handle().clone();
                let server = server.clone();
                let account_id = account_id.clone();
                tauri::async_runtime::spawn(async move {
                    let result = pomme_launcher::auto_benchmark::run(
                        handle.clone(),
                        &server,
                        account_id.as_deref(),
                    )
                    .await;
                    match result {
                        Ok(path) => {
                            println!("{}", path.display());
                            handle.exit(0);
                        }
                        Err(reason) => {
                            println!("{reason}");
                            handle.exit(1);
                        }
                    }
                });
            }
            Ok(())
        })
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .invoke_handler(invoke_handler)
        .run(tauri::generate_context!())
        .expect("failed to run Pomme launcher");
}
