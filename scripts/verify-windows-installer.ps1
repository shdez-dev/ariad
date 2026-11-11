param(
    [Parameter(Mandatory = $true)]
    [string]$InstallerPath,

    [Parameter(Mandatory = $true)]
    [string]$EvidencePath
)

$ErrorActionPreference = "Stop"

if ($env:GITHUB_ACTIONS -ne "true" -or $env:RUNNER_ENVIRONMENT -ne "github-hosted") {
    throw "La prueba de ciclo de vida requiere un runner Windows hospedado y efímero de GitHub Actions."
}

if ([string]::IsNullOrWhiteSpace($env:RUNNER_TEMP) -or -not (Test-Path -LiteralPath $env:RUNNER_TEMP -PathType Container)) {
    throw "RUNNER_TEMP no está disponible; no se puede aislar la instalación de prueba."
}

$resolvedTemp = [System.IO.Path]::GetFullPath($env:RUNNER_TEMP)
$runId = [guid]::NewGuid().ToString("N")
$testRoot = Join-Path $resolvedTemp "ariadna-ard-22-$runId"
$installDirectory = Join-Path $testRoot "install"
$windowsAppData = [Environment]::GetFolderPath([Environment+SpecialFolder]::ApplicationData)
if ([string]::IsNullOrWhiteSpace($windowsAppData) -or -not (Test-Path -LiteralPath $windowsAppData -PathType Container)) {
    throw "Windows no devolvió una carpeta ApplicationData válida para el runner."
}
$applicationDataDirectory = Join-Path $windowsAppData "com.ariadna.inventory"
$databasePath = Join-Path $applicationDataDirectory "ariadna.sqlite3"
$logPath = Join-Path $applicationDataDirectory "logs\ariadna.log"
$appProcess = $null
$result = "Fallida"
$failure = $null
$databaseCreated = $false
$installationPassed = $false
$startupLogged = $false
$relaunchPassed = $false
$uninstallPassed = $false
$appDataRetained = $false
$installerHash = "No disponible"
$signatureStatus = "No disponible"
$osCaption = (Get-CimInstance Win32_OperatingSystem).Caption
$osVersion = (Get-CimInstance Win32_OperatingSystem).Version
$revision = (& git rev-parse HEAD).Trim()

try {
    if (Test-Path -LiteralPath $applicationDataDirectory) {
        throw "El perfil del runner no está limpio: ya existe el directorio de datos de Ariadna."
    }

    $resolvedInstaller = (Resolve-Path -LiteralPath $InstallerPath).Path
    $installerHash = (Get-FileHash -LiteralPath $resolvedInstaller -Algorithm SHA256).Hash
    $signatureStatus = (Get-AuthenticodeSignature -LiteralPath $resolvedInstaller).Status.ToString()

    if ($signatureStatus -ne "NotSigned") {
        throw "Se esperaba un instalador de demostración sin firma; estado detectado: $signatureStatus."
    }

    New-Item -ItemType Directory -Path $installDirectory -Force | Out-Null

    $install = Start-Process -FilePath $resolvedInstaller -ArgumentList @("/S", "/D=$installDirectory") -Wait -PassThru
    if ($install.ExitCode -ne 0) {
        throw "El instalador NSIS terminó con código $($install.ExitCode)."
    }

    $applicationPath = Join-Path $installDirectory "Ariadna.exe"
    if (-not (Test-Path -LiteralPath $applicationPath -PathType Leaf)) {
        throw "No se encontró Ariadna.exe en el directorio de instalación indicado."
    }
    $installationPassed = $true

    $appProcess = Start-Process -FilePath $applicationPath -PassThru
    $deadline = [DateTime]::UtcNow.AddSeconds(45)
    while ([DateTime]::UtcNow -lt $deadline) {
        $appProcess.Refresh()
        if ($appProcess.HasExited) {
            throw "Ariadna se cerró durante el primer inicio (código $($appProcess.ExitCode))."
        }

        if ((Test-Path -LiteralPath $databasePath -PathType Leaf) -and (Test-Path -LiteralPath $logPath -PathType Leaf)) {
            $logContents = Get-Content -LiteralPath $logPath -Raw
            if ($logContents -match '"command":"app\.startup","result":"success"') {
                $databaseCreated = (Get-Item -LiteralPath $databasePath).Length -gt 0
                $startupLogged = $databaseCreated
                if ($startupLogged) { break }
            }
        }

        Start-Sleep -Seconds 1
    }

    if (-not $startupLogged) {
        throw "No se observó la creación de la base local y el registro de inicio exitoso en 45 segundos."
    }

    if (-not $appProcess.CloseMainWindow()) {
        Stop-Process -Id $appProcess.Id -Force
    } elseif (-not $appProcess.WaitForExit(10000)) {
        Stop-Process -Id $appProcess.Id -Force
    }
    $appProcess.Dispose()
    $appProcess = $null

    $startupCountBeforeRelaunch = 0
    if (Test-Path -LiteralPath $logPath -PathType Leaf) {
        $logBeforeRelaunch = Get-Content -LiteralPath $logPath -Raw
        $startupCountBeforeRelaunch = [regex]::Matches($logBeforeRelaunch, '"command":"app\.startup","result":"success"').Count
    }

    $appProcess = Start-Process -FilePath $applicationPath -PassThru
    $relaunchDeadline = [DateTime]::UtcNow.AddSeconds(30)
    while ([DateTime]::UtcNow -lt $relaunchDeadline) {
        $appProcess.Refresh()
        if ($appProcess.HasExited) {
            throw "Ariadna se cerró durante el segundo inicio (código $($appProcess.ExitCode))."
        }
        if ((Test-Path -LiteralPath $databasePath -PathType Leaf) -and (Test-Path -LiteralPath $logPath -PathType Leaf)) {
            $logAfterRelaunch = Get-Content -LiteralPath $logPath -Raw
            $startupCountAfterRelaunch = [regex]::Matches($logAfterRelaunch, '"command":"app\.startup","result":"success"').Count
            if ($startupCountAfterRelaunch -gt $startupCountBeforeRelaunch) {
                $relaunchPassed = $true
                break
            }
        }
        Start-Sleep -Seconds 1
    }

    if (-not $relaunchPassed) {
        throw "Ariadna no pudo volver a iniciar con su base local existente."
    }

    if (-not $appProcess.CloseMainWindow()) {
        Stop-Process -Id $appProcess.Id -Force
    } elseif (-not $appProcess.WaitForExit(10000)) {
        Stop-Process -Id $appProcess.Id -Force
    }
    $appProcess.Dispose()
    $appProcess = $null

    $uninstallerPath = Join-Path $installDirectory "uninstall.exe"
    if (-not (Test-Path -LiteralPath $uninstallerPath -PathType Leaf)) {
        throw "No se encontró el desinstalador NSIS."
    }

    $uninstall = Start-Process -FilePath $uninstallerPath -ArgumentList "/S" -Wait -PassThru
    if ($uninstall.ExitCode -ne 0) {
        throw "El desinstalador NSIS terminó con código $($uninstall.ExitCode)."
    }

    $uninstallDeadline = [DateTime]::UtcNow.AddSeconds(15)
    while ([DateTime]::UtcNow -lt $uninstallDeadline -and (Test-Path -LiteralPath $applicationPath)) {
        Start-Sleep -Milliseconds 250
    }
    $uninstallPassed = -not (Test-Path -LiteralPath $applicationPath)
    if (-not $uninstallPassed) {
        throw "La aplicación continúa presente después de ejecutar el desinstalador."
    }

    $appDataRetained = (Test-Path -LiteralPath $databasePath -PathType Leaf) -and (Test-Path -LiteralPath $logPath -PathType Leaf)
    $result = "Correcta"
} catch {
    $failure = $_.Exception.Message
    throw
} finally {
    if ($null -ne $appProcess) {
        try {
            $appProcess.Refresh()
            if (-not $appProcess.HasExited) {
                Stop-Process -Id $appProcess.Id -Force
            }
        } catch {
            Write-Warning "No se pudo cerrar el proceso de prueba: $($_.Exception.Message)"
        }
        $appProcess.Dispose()
    }

    $evidenceDirectory = Split-Path -Parent $EvidencePath
    if (-not [string]::IsNullOrWhiteSpace($evidenceDirectory)) {
        New-Item -ItemType Directory -Path $evidenceDirectory -Force | Out-Null
    }

    $failureLine = if ($null -eq $failure) { "Sin errores." } else { $failure.Replace("`r", " ").Replace("`n", " ") }
    $databaseState = if ($databaseCreated) { "Creada y con tamaño mayor que cero." } else { "No confirmada." }
    $startupState = if ($startupLogged) { "Correcto; evento app.startup registrado." } else { "No confirmado." }
    $relaunchState = if ($relaunchPassed) { "Correcto con la base existente." } else { "No confirmado." }
    $uninstallState = if ($uninstallPassed) { "Correcta; Ariadna.exe dejó de existir en el directorio de prueba." } else { "No confirmada." }
    $dataRetentionState = if ($appDataRetained) {
        "La base y el registro permanecieron después de desinstalar."
    } elseif ((Test-Path -LiteralPath $databasePath) -or (Test-Path -LiteralPath $logPath)) {
        "Solo uno de los dos archivos permaneció; consultar el perfil efímero del runner."
    } else {
        "La base y el registro no permanecieron después de desinstalar."
    }

    $evidence = @"
# Evidencia ARD-22: instalador NSIS de Ariadna

Resultado: **$result**

Commit probado: ``$revision``

Runner Windows: $osCaption ($osVersion)

Instalador: ``$(Split-Path -Leaf $InstallerPath)``

SHA-256: ``$installerHash``

Firma Authenticode: **$signatureStatus**

Directorio de instalación de prueba: ``RUNNER_TEMP/ariadna-ard-22-*/install``

Perfil de datos comprobado: ``%APPDATA%\com.ariadna.inventory`` en runner hospedado limpio

## Verificaciones

1. Instalación silenciosa NSIS: $(if ($installationPassed) { "correcta" } else { "no confirmada" }).
2. Primer inicio y creación de base: $startupState $databaseState
3. Segundo inicio con base existente: $relaunchState
4. Desinstalación silenciosa NSIS: $uninstallState
5. Datos locales después de desinstalar: $dataRetentionState

Base de datos esperada: ``%APPDATA%\com.ariadna.inventory\ariadna.sqlite3``

Registro esperado: ``%APPDATA%\com.ariadna.inventory\logs\ariadna.log``
Firma y advertencias: el artefacto de esta etapa está **sin firma**. Windows puede mostrar advertencias de reputación al descargarlo; Smart App Control puede bloquear aplicaciones desconocidas sin firma.

Detalle del resultado: $failureLine

Esta verificación se ejecuta en un runner Windows efímero de GitHub Actions. No usa el perfil de la máquina de desarrollo.
"@

    Set-Content -LiteralPath $EvidencePath -Value $evidence -Encoding utf8
    Write-Host "Evidencia ARD-22: $EvidencePath"
}

if ($result -ne "Correcta") {
    throw "La verificación ARD-22 no terminó correctamente."
}
