// This registry is included by both build.rs and src/lib.rs. Keeping command
// identifiers here makes Tauri's generated ACL and runtime invoke handler
// mechanically share one source of truth.
macro_rules! dashy_app_commands {
    ($receiver:ident) => {
        $receiver! {
            get_dashboard_snapshot,
            refresh_dashboard_provider,
            get_provider_setup_states,
            install_provider,
            login_provider,
            complete_onboarding,
            get_settings,
            get_current_edge_view,
            update_settings,
            set_notch_interaction,
            begin_notch_exit,
            complete_notch_exit,
            list_monitors,
            show_notch_menu,
            open_settings,
            set_tray_labels,
        }
    };
}
