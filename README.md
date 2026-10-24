# Lite Music
YouTube Music en una ventana ligera (Tauri + WebView2), con un reproductor flotante pixel art (perro, gato, carátula o solo controles). Cerrar la ventana la oculta a la bandeja.

## Instalar (Windows)
1. Entra en **Releases** → la versión más reciente → descarga `Lite Music_..._x64-setup.exe`.
2. Ejecútalo. Si Windows muestra "Windows protegió su PC": **Más información → Ejecutar de todos modos** (el instalador no está firmado con certificado comercial).
3. Se actualiza sola: al abrirse busca versiones nuevas (también en la bandeja → "Buscar actualizaciones").

## Para el autor
- Cada push a `main` publica una versión nueva (`0.1.N`) con el instalador y `latest.json` firmado.
- Secretos del repositorio: `TAURI_SIGNING_PRIVATE_KEY` (y `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, vacío si la clave no tiene contraseña).
