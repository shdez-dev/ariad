# Operación, copias y restauración

## Alcance

Ariadna es un prototipo de escritorio que guarda los datos localmente. Esta guía describe una copia manual del archivo SQLite y su restauración. La aplicación no crea copias automáticas, no sincroniza con la nube y no cifra los respaldos. Protege la carpeta de respaldo como protegerías los datos del inventario y no compartas copias con información privada.

## Ubicación de los datos

En Windows, la base de datos es:

`%APPDATA%\com.ariadna.inventory\ariadna.sqlite3`

El registro de operación se guarda en `%APPDATA%\com.ariadna.inventory\logs\ariadna.log`. El archivo de base de datos contiene el inventario, productos, pallets, ubicaciones, movimientos y alertas.

SQLite usa el modo WAL. Mientras Ariadna está abierta, algunos cambios pueden estar en `ariadna.sqlite3-wal` y todavía no en el archivo principal. No copies únicamente `ariadna.sqlite3` mientras el proceso tenga la base abierta. Cierra Ariadna y espera a que `ariadna.exe` termine antes de copiar.

## Crear y comprobar una copia

1. Cierra Ariadna desde su ventana.
2. Abre PowerShell y comprueba que no quede un proceso de la aplicación:

```powershell
$ariadnaProcess = Get-Process -Name ariadna -ErrorAction SilentlyContinue
if ($ariadnaProcess) { throw "Cierra Ariadna antes de copiar la base de datos." }
```

3. Copia la base a una carpeta separada del directorio de datos. El ejemplo crea una carpeta bajo Documentos y un nombre con fecha y hora:

```powershell
$ariadnaDatabase = Join-Path $env:APPDATA "com.ariadna.inventory\ariadna.sqlite3"
$ariadnaBackupDirectory = Join-Path $env:USERPROFILE "Documents\AriadnaBackups"
if (-not (Test-Path -LiteralPath $ariadnaDatabase)) { throw "No se encontró la base de datos de Ariadna." }
New-Item -ItemType Directory -Force -Path $ariadnaBackupDirectory | Out-Null
$ariadnaBackup = Join-Path $ariadnaBackupDirectory ("ariadna-" + (Get-Date -Format "yyyyMMdd-HHmmss") + ".sqlite3")
Copy-Item -LiteralPath $ariadnaDatabase -Destination $ariadnaBackup
Get-FileHash -LiteralPath $ariadnaBackup -Algorithm SHA256
```

4. Conserva el archivo y su hash. Mantén más de una copia si los datos importan; copiar en el mismo disco no protege frente a una avería de ese disco.

Si Ariadna se cerró de forma inesperada, no elimines ni separes los archivos `ariadna.sqlite3-wal` y `ariadna.sqlite3-shm` antes de una recuperación. Conserva juntos los archivos de la misma base y busca asistencia antes de reemplazarlos; no combines archivos auxiliares de fechas diferentes.

## Restaurar una copia

Restaurar reemplaza el inventario local actual. Antes de hacerlo, confirma la ruta del respaldo y conserva una copia de seguridad de la base actual.

1. Cierra Ariadna y confirma que `ariadna.exe` ya no está en ejecución.
2. Identifica una copia de respaldo conocida y conserva su hash. No la abras ni la edites directamente.
3. Guarda una copia de la base actual y coloca el respaldo en la ruta de datos:

```powershell
$ariadnaProcess = Get-Process -Name ariadna -ErrorAction SilentlyContinue
if ($ariadnaProcess) { throw "Cierra Ariadna antes de restaurar la base de datos." }
$ariadnaDataDirectory = Join-Path $env:APPDATA "com.ariadna.inventory"
$ariadnaDatabase = Join-Path $ariadnaDataDirectory "ariadna.sqlite3"
$ariadnaBackup = "C:\ruta\al\respaldo\ariadna-AAAAMMDD-HHMMSS.sqlite3"
if (-not (Test-Path -LiteralPath $ariadnaBackup)) { throw "No se encontró el respaldo indicado." }
$ariadnaTimestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$ariadnaRecoveryDirectory = Join-Path $ariadnaDataDirectory ("ariadna-recuperacion-" + $ariadnaTimestamp)
$ariadnaRestoreStaging = Join-Path $ariadnaDataDirectory ("ariadna-restaurada-" + $ariadnaTimestamp + ".sqlite3")
$ariadnaExpectedHash = (Get-FileHash -LiteralPath $ariadnaBackup -Algorithm SHA256).Hash
New-Item -ItemType Directory -Path $ariadnaRecoveryDirectory | Out-Null
Copy-Item -LiteralPath $ariadnaBackup -Destination $ariadnaRestoreStaging
$ariadnaStagingHash = (Get-FileHash -LiteralPath $ariadnaRestoreStaging -Algorithm SHA256).Hash
if ($ariadnaStagingHash -ne $ariadnaExpectedHash) { throw "El hash de la copia preparada no coincide; no reemplaces la base actual." }
foreach ($ariadnaSidecarSuffix in @("-wal", "-shm")) {
    $ariadnaSidecarPath = $ariadnaDatabase + $ariadnaSidecarSuffix
    if (Test-Path -LiteralPath $ariadnaSidecarPath) { Move-Item -LiteralPath $ariadnaSidecarPath -Destination $ariadnaRecoveryDirectory }
}
if (Test-Path -LiteralPath $ariadnaDatabase) { Move-Item -LiteralPath $ariadnaDatabase -Destination $ariadnaRecoveryDirectory }
Move-Item -LiteralPath $ariadnaRestoreStaging -Destination $ariadnaDatabase
Get-FileHash -LiteralPath $ariadnaDatabase -Algorithm SHA256
```

Reemplaza `C:\ruta\al\respaldo\ariadna-AAAAMMDD-HHMMSS.sqlite3` por la ruta real del respaldo antes de ejecutar el bloque. El bloque prepara una copia temporal, compara su SHA-256 con el respaldo y solo entonces aparta la base actual y sus archivos `-wal`/`-shm` en una carpeta de recuperación fechada. No borres esa carpeta hasta confirmar la restauración.

4. Inicia Ariadna. Comprueba que abre sin error y que aparecen productos, ubicaciones, pallets e historial que esperas recuperar. La aplicación aplica las migraciones pendientes al iniciar.
5. Si los datos no son los esperados o la aplicación no inicia, ciérrala. Devuelve `ariadna.sqlite3` desde la carpeta `ariadna-recuperacion-...` al directorio de datos y devuelve junto a él cualquier `ariadna.sqlite3-wal` y `ariadna.sqlite3-shm` que se haya apartado; luego vuelve a iniciar Ariadna.

## Antes de actualizar la aplicación

1. Crea una copia siguiendo esta guía antes de iniciar una versión nueva.
2. Conserva el instalador o la revisión de Git compatible con la copia.
3. La base puede recibir migraciones al iniciar una versión nueva. No asumas que una base migrada por una versión más reciente puede abrirse con una versión antigua. Para recuperar datos, usa la versión que creó la copia o una versión posterior compatible.
4. No interrumpas Ariadna mientras se esté iniciando o aplicando migraciones.

## Validación y límites

La prueba automatizada `a_restored_backup_passes_integrity_and_keeps_inventory` utiliza una base temporal: inserta un producto, ejecuta y comprueba el checkpoint WAL, cierra SQLite, crea una copia, la restaura en otra ruta, vuelve a abrirla con las migraciones de Ariadna y verifica `PRAGMA integrity_check` y la presencia del producto. Esa prueba valida el procedimiento de archivo en un entorno aislado. La sustitución y recuperación manual del perfil real de Windows no se ha ejecutado; no se presenta como una restauración manual validada.

Las copias son manuales y locales. No hay retención automática, cifrado del archivo, respaldo remoto ni restauración granular de un producto o movimiento.
