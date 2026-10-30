fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(
            tauri_build::AppManifest::new().commands(&["report_state", "get_state", "control", "scale_float", "get_settings", "set_setting", "settings_action", "choose_source", "open_settings_win", "set_skin_cmd", "open_link"]),
        ),
    )
    .expect("error al generar la configuración de Tauri");
}
