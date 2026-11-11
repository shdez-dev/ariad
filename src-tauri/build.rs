const APP_COMMANDS: &[&str] = &[
    "create_product",
    "get_product",
    "list_products",
    "search_inventory",
    "update_product",
    "deactivate_product",
    "create_location",
    "get_location",
    "list_locations",
    "update_location",
    "deactivate_location",
    "get_handling_unit",
    "list_handling_units",
    "receive_unit",
    "store_unit",
    "move_unit",
    "dispatch_unit",
    "record_movement_correction",
    "get_movement_history",
    "list_alerts",
    "mark_alert_reviewed",
    "run_simulation",
];

fn main() {
    let attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(APP_COMMANDS));
    tauri_build::try_build(attributes).expect("failed to run tauri-build");
}
