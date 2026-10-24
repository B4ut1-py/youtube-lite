# Lite Music
YouTube Music en una ventana ligera (Tauri + WebView2). Cerrar la ventana la oculta a la bandeja.

## Compilar en Windows
Requisitos: Rust (rustup), Node 22.
    npm install
    npm run tauri icon icon.png
    npm run tauri build
Instalador: src-tauri/target/release/bundle/nsis/

## Sin instalar nada
Sube la carpeta a GitHub: la acción "Build Windows" genera el instalador como artefacto.
