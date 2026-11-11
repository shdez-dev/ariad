# Instalador de demostración para Windows

ARD-22 configura un instalador NSIS para la aplicación de escritorio Ariadna. La etapa genera un artefacto identificable desde un commit del repositorio y valida su ciclo de vida en un runner Windows efímero de GitHub Actions; los mismos comandos permiten repetir el empaquetado.

## Alcance

- Producto: Ariadna 0.1.0.
- Plataforma validada: Windows x64 con `x86_64-pc-windows-msvc`.
- Formato de distribución de esta etapa: setup NSIS (`*-setup.exe`). MSI y otros sistemas operativos quedan fuera.
- Datos de demostración: la aplicación no carga inventario de una empresa ni usa hardware real.
- Firma: el artefacto no está firmado. La compilación no requiere certificado, pero una distribución pública puede mostrar advertencias de reputación de Windows. Smart App Control puede bloquear ejecutables desconocidos sin firma.
- WebView2: la interfaz de Tauri lo utiliza para renderizar la aplicación. La instalación puede requerir conexión para obtenerlo si el equipo no tiene el runtime.

## Ubicaciones locales

En Windows, Ariadna guarda su base y los registros bajo el directorio de datos de la aplicación del usuario:

- Base SQLite: `%APPDATA%\com.ariadna.inventory\ariadna.sqlite3`
- Registro: `%APPDATA%\com.ariadna.inventory\logs\ariadna.log`
- Instalación NSIS: por usuario, en la ubicación elegida por el instalador.

La instalación no usa un servicio ni requiere una base de datos externa. El desinstalador quita la aplicación; los datos del usuario deben tratarse como datos separados y respaldarse antes de desinstalar o reinstalar. El comportamiento observado sobre la retención de datos se anota en la evidencia de cada ejecución.

## Generar el artefacto

Requisitos del equipo de compilación: Windows x64, Rust estable con MSVC, Microsoft C++ Build Tools, Node.js/npm y WebView2 para ejecutar la aplicación. Desde la raíz del repositorio:

```powershell
npm ci
npm run tauri -- build
```

El instalador queda en `src-tauri/target/release/bundle/nsis/`. Para identificarlo, registra el commit de origen y su SHA-256:

```powershell
git rev-parse HEAD
Get-FileHash src-tauri/target/release/bundle/nsis/*-setup.exe -Algorithm SHA256
Get-AuthenticodeSignature src-tauri/target/release/bundle/nsis/*-setup.exe
```

La configuración actual no declara certificado ni comando de firma. En consecuencia, el estado esperado de Authenticode es `NotSigned`.

## Verificación de CI

El workflow `.github/workflows/ci.yml` ejecuta `scripts/verify-windows-installer.ps1` en `windows-latest`. El script se niega a ejecutarse fuera de un runner hospedado de GitHub Actions, crea un directorio de instalación único bajo `RUNNER_TEMP` y exige que el perfil efímero esté limpio. Ejecuta el instalador NSIS de Ariadna en modo silencioso, inicia Ariadna, espera la base SQLite y el registro de inicio exitoso, vuelve a iniciar la aplicación y la desinstala. La evidencia registra el commit, hash, estado de firma y resultado de cada paso sin guardar datos del inventario.

El artefacto `ariadna-windows-nsis-<commit>` contiene el instalador y el informe de verificación durante 14 días. Se descarga desde la sección **Artifacts** de la ejecución del workflow de la rama o pull request. Si un paso de instalación falla, el workflow conserva el artefacto de compilación disponible y el informe del fallo para diagnóstico.

Esta prueba automática verifica instalación, arranque y desinstalación en un Windows limpio. No sustituye la aceptación visual manual de la interfaz prevista en ARD-11 a ARD-15 ni garantiza compatibilidad con equipos administrados, políticas corporativas o todas las configuraciones de Smart App Control.

## Referencias oficiales

- [Instaladores Windows de Tauri](https://v2.tauri.app/distribute/windows-installer/)
- [Prerrequisitos de Tauri para Windows](https://v2.tauri.app/start/prerequisites/)
- [Firma de código de Windows en Tauri](https://v2.tauri.app/distribute/sign/windows/)
