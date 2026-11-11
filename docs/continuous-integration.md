# Integración continua

ARD-20 configura el workflow `.github/workflows/ci.yml` para validar los cambios en GitHub Actions.

## Ejecución

El workflow se ejecuta al publicar cualquier rama, abrir o actualizar un pull request y solicitar una ejecución manual. El job `Windows` usa `windows-latest`, Node.js 24 y Rust stable con el target `x86_64-pc-windows-msvc`. Solo requiere permiso de lectura del contenido del repositorio y no usa secretos.

El job instala dependencias con `npm ci`, compila la interfaz, ejecuta las cuatro suites frontend, verifica `cargo fmt`, Clippy y las pruebas Rust, comprueba la estadística de percentiles del benchmark de ARD-21 y compila la aplicación Tauri con `--no-bundle`. Esto comprueba el ejecutable de escritorio sin producir un instalador ni validar su firma.

El resultado aparece como el chequeo `CI / Windows`. Un fallo deja el chequeo en estado fallido para esa rama o pull request. Si se desea impedir fusiones mientras falle, la regla de protección de ramas de GitHub debe marcar `CI / Windows` como chequeo requerido.

## Comandos locales

Desde la raíz del repositorio:

```powershell
npm ci
npm run build
npm run test:map
npm run test:catalog
npm run test:simulator
npm run test:ipc
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --example ard21_benchmark
npm run tauri -- build --no-bundle
```

La compilación de Tauri genera un ejecutable Windows sin instalador. El empaquetado, instalación, desinstalación y firma se verifican por separado en ARD-22.
