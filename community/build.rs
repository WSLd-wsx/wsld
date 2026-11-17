fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "probe_note", "probe_close", "store_environment", "store_open_listing",
            "store_activate_edition", "store_entitlement", "store_purchase",
            "store_check_updates", "store_download_update", "online_releases",
            "install_release", "install_progress", "inventory", "launch_distribution",
        ]),
    )).expect("Community GUI resource build failed");
}
